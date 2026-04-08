// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/cap/mod.rs

//! Capability subsystem (Phase 7).
//!
//! Initialised by [`init_capability_system`], which creates the root `CSpace`
//! (id 0) populated with initial capabilities for all boot-provided hardware
//! resources:
//!
//! - Usable physical memory → [`CapTag::Frame`] caps (MAP | WRITE)
//! - MMIO ranges (`MmioRange`, `PciEcam`, `IommuUnit`) → [`CapTag::MmioRegion`] caps (MAP)
//! - Interrupt lines → [`CapTag::Interrupt`] caps
//! - Firmware tables → [`CapTag::Frame`] caps (MAP only, no WRITE)
//! - I/O port ranges (x86-64) → [`CapTag::IoPortRange`] caps (USE)
//! - One [`CapTag::SchedControl`] cap (ELEVATE)
//!
//! The populated `CSpace` is stored in [`ROOT_CSPACE`] until Phase 9 hands it
//! to the init process.

// cast_possible_truncation: u64→usize/u32/u16 capability field extractions bounded by capability space.
#![allow(clippy::cast_possible_truncation)]

extern crate alloc;

use alloc::boxed::Box;


pub mod cspace;
pub mod derivation;
pub mod object;
pub mod slot;

// Re-exports for convenience. Many are consumed by future phases; suppress the
// unused lint rather than removing symbols that future code will reference.
#[allow(unused_imports)]
pub use cspace::{CSpace, CapError, L1_SIZE, L2_SIZE};
#[allow(unused_imports)]
pub use derivation::DERIVATION_LOCK;
#[allow(unused_imports)]
pub use object::{
    AddressSpaceObject, CSpaceKernelObject, EndpointObject, FrameObject, InterruptObject,
    IoPortRangeObject, KernelObjectHeader, MmioRegionObject, ObjectType, SchedControlObject,
    SignalObject, ThreadObject,
};
#[allow(unused_imports)]
pub use slot::{CSpaceId, CapTag, CapabilitySlot, Rights, SlotId};

use boot_protocol::{BootInfo, MemoryMapEntry, MemoryType, PlatformResource, ResourceType};
use core::ptr::NonNull;
use core::sync::atomic::{AtomicPtr, AtomicU32, Ordering};

use crate::mm::paging::phys_to_virt;

// ── Globals ───────────────────────────────────────────────────────────────────

/// Root capability space, populated during Phase 7.
///
/// Consumed (transferred to init) during Phase 9. Access is single-threaded
/// during boot; `static mut` is safe here under that invariant.
#[cfg(not(test))]
pub static mut ROOT_CSPACE: Option<Box<CSpace>> = None;

/// Take the root `CSpace` out of `ROOT_CSPACE`, leaving `None`.
///
/// Uses raw pointer operations to avoid creating a mutable reference to a
/// mutable static (which is undefined behaviour in concurrent contexts and
/// warned by `static_mut_refs`). Safe here because access is single-threaded
/// during boot.
///
/// # Safety
/// Must be called only in single-threaded boot context before init runs.
#[cfg(not(test))]
pub unsafe fn take_root_cspace() -> Option<Box<CSpace>>
{
    // SAFETY: single-threaded boot; no concurrent access.
    let ptr = core::ptr::addr_of_mut!(ROOT_CSPACE);
    core::ptr::replace(ptr, None)
}

/// Borrow the root `CSpace` mutably.
///
/// Uses raw pointer operations to avoid creating a mutable reference to a
/// mutable static. Safe here because access is single-threaded during boot.
///
/// # Safety
/// Must be called only in single-threaded boot context before init runs.
#[cfg(not(test))]
pub unsafe fn root_cspace_mut() -> Option<&'static mut CSpace>
{
    // SAFETY: single-threaded boot; no concurrent access.
    let ptr = core::ptr::addr_of_mut!(ROOT_CSPACE);
    unsafe { (*ptr).as_mut().map(Box::as_mut) }
}

/// Monotonically increasing `CSpace` ID allocator. Root gets ID 0.
static NEXT_CSPACE_ID: AtomicU32 = AtomicU32::new(0);

/// Maximum number of live `CSpaces.` Sized for practical OS use.
const MAX_CSPACES: usize = 4096;

/// Global registry mapping `CSpaceId` → raw *mut `CSpace.`
///
/// Populated by [`register_cspace`] when a `CSpace` is created, cleared by
/// [`unregister_cspace`] when the backing object is freed. Required for
/// derivation tree traversal: `SlotId` stores a `CSpaceId`, and we need
/// O(1) resolution to the actual `CSpace` to read/write derivation pointers.
///
/// # Safety invariant
/// A non-null entry is valid as long as the corresponding `CSpaceKernelObject`
/// refcount is > 0. After `dec_ref` reaches 0 and `dealloc_object` runs,
/// `unregister_cspace` clears the entry before the memory is freed.
// SAFETY: AtomicPtr<CSpace> is Send+Sync; array-of-atomics is always valid
// for static initialisation.
static CSPACE_REGISTRY: [AtomicPtr<CSpace>; MAX_CSPACES] = {
    // AtomicPtr has no const Default impl, so we must use a const block with
    // a fixed-size initialiser. The array literal approach requires all
    // elements to be const-evaluable; `AtomicPtr::new(null_mut())` is const.
    // Rust does not allow `[expr; N]` when N > 32 for non-Copy types in stable,
    // so we use a transmute from a zero-initialised array of usize instead.
    //
    // SAFETY: AtomicPtr<T> is repr(transparent) over *mut T (a pointer-sized
    // integer), so a zero-initialised array of pointer-sized words is a valid
    // array of null AtomicPtr values.
    unsafe {
        core::mem::transmute::<[usize; MAX_CSPACES], [AtomicPtr<CSpace>; MAX_CSPACES]>(
            [0usize; MAX_CSPACES],
        )
    }
};

/// Register a `CSpace` pointer under its ID.
///
/// Called immediately after a [`CSpace`] is heap-allocated. Panics (in debug)
/// or silently drops (in release) if `id >= MAX_CSPACES`.
pub fn register_cspace(id: CSpaceId, ptr: *mut CSpace)
{
    if (id as usize) < MAX_CSPACES
    {
        CSPACE_REGISTRY[id as usize].store(ptr, Ordering::Release);
    }
}

/// Clear a `CSpace` registration.
///
/// Called from `dealloc_object` for `ObjectType::CSpaceObj` *before* freeing
/// the backing allocation, so no dangling pointer is observable.
pub fn unregister_cspace(id: CSpaceId)
{
    if (id as usize) < MAX_CSPACES
    {
        CSPACE_REGISTRY[id as usize].store(core::ptr::null_mut(), Ordering::Release);
    }
}

/// Resolve a `CSpaceId` to a raw pointer.
///
/// Returns `None` if `id` is out of range or not yet registered. The returned
/// pointer is valid only while the corresponding `CSpaceKernelObject` has a
/// positive refcount and `DERIVATION_LOCK` is held by the caller.
pub fn lookup_cspace(id: CSpaceId) -> Option<*mut CSpace>
{
    if (id as usize) >= MAX_CSPACES
    {
        return None;
    }
    let ptr = CSPACE_REGISTRY[id as usize].load(Ordering::Acquire);
    if ptr.is_null()
    {
        None
    }
    else
    {
        Some(ptr)
    }
}

/// Allocate a unique `CSpace` ID.
///
/// Called by `SYS_CAP_CREATE_CSPACE` when creating new `CSpace` objects at
/// runtime. The root `CSpace` is assigned ID 0 at init time via this same
/// counter.
pub fn alloc_cspace_id() -> CSpaceId
{
    NEXT_CSPACE_ID.fetch_add(1, Ordering::Relaxed)
}

/// Maximum slots in the root `CSpace` (full two-level directory).
const ROOT_CSPACE_MAX_SLOTS: usize = 16384;

// ── Phase 7 entry point ───────────────────────────────────────────────────────

/// Initialise the capability system and populate the root `CSpace.`
///
/// `platform_resources` is the validated list from Phase 6. `boot_info_phys`
/// is the physical address of the [`BootInfo`] structure; re-derived here via
/// the direct physical map (active since Phase 3) to access the memory map.
///
/// Returns the number of capability slots populated. Calls [`crate::fatal`] on
/// any allocation failure.
///
/// # Safety
///
/// Must be called exactly once, single-threaded, after Phase 4 (heap active)
/// and Phase 3 (direct map active).
pub fn init_capability_system(
    platform_resources: &[PlatformResource],
    boot_info_phys: u64,
) -> usize
{
    let id = NEXT_CSPACE_ID.fetch_add(1, Ordering::Relaxed);
    let mut cspace = Box::new(CSpace::new(id, ROOT_CSPACE_MAX_SLOTS));

    // Re-derive BootInfo via the direct physical map to access the memory map.
    // SAFETY: boot_info_phys was validated in Phase 0; direct map active since Phase 3.
    let info: &BootInfo = unsafe { &*(phys_to_virt(boot_info_phys) as *const BootInfo) };

    // Build memory map slice.
    let mmap: &[MemoryMapEntry] = if info.memory_map.count == 0 || info.memory_map.entries.is_null()
    {
        &[]
    }
    else
    {
        // SAFETY: Phase 0 confirmed memory_map is valid; direct map active.
        unsafe {
            core::slice::from_raw_parts(
                phys_to_virt(info.memory_map.entries as u64) as *const MemoryMapEntry,
                info.memory_map.count as usize,
            )
        }
    };

    populate_cspace(&mut cspace, mmap, platform_resources);

    let count = cspace.populated_count();

    // Store in ROOT_CSPACE (kernel runtime only; test builds skip this).
    #[cfg(not(test))]
    // SAFETY: single-threaded boot; ROOT_CSPACE not yet accessed.
    unsafe {
        let raw = core::ptr::addr_of_mut!(*cspace);
        register_cspace(id, raw);
        ROOT_CSPACE = Some(cspace);
    }
    // In test mode the box is dropped here — kernel objects are leaked
    // intentionally via Box::into_raw in nonnull_from_box, which is
    // acceptable for isolated unit tests.
    #[cfg(test)]
    let _ = cspace;

    count
}

/// Core `CSpace` population logic, separated for testability.
///
/// Creates one capability per usable memory region, per platform resource,
/// and one `SchedControl` capability.
// too_many_lines: one logical pass over all boot-time resource types; splitting
// would require threading shared state (cspace) through multiple helper functions.
#[allow(clippy::too_many_lines)]
fn populate_cspace(
    cspace: &mut CSpace,
    mmap: &[MemoryMapEntry],
    platform_resources: &[PlatformResource],
)
{
    // Usable physical memory → Frame caps with MAP | WRITE.
    for entry in mmap
    {
        if entry.memory_type != MemoryType::Usable
        {
            continue;
        }
        let obj = Box::new(FrameObject {
            header: KernelObjectHeader::new(ObjectType::Frame),
            base: entry.physical_base,
            size: entry.size,
        });
        let ptr = nonnull_from_box(obj);
        insert_or_fatal(
            cspace,
            CapTag::Frame,
            Rights::MAP | Rights::WRITE,
            ptr,
            "Phase 7: cannot allocate Frame capability for usable memory",
        );
    }

    // Platform resources → type-specific capabilities.
    for res in platform_resources
    {
        match res.resource_type
        {
            // MMIO regions: MAP only (no WRITE — drivers map via devmgr).
            ResourceType::MmioRange | ResourceType::PciEcam | ResourceType::IommuUnit =>
            {
                let obj = Box::new(MmioRegionObject {
                    header: KernelObjectHeader::new(ObjectType::MmioRegion),
                    base: res.base,
                    size: res.size,
                    flags: res.flags,
                    _pad: 0,
                });
                let ptr = nonnull_from_box(obj);
                insert_or_fatal(
                    cspace,
                    CapTag::MmioRegion,
                    Rights::MAP,
                    ptr,
                    "Phase 7: cannot allocate MmioRegion capability",
                );
            }

            // Firmware tables: Frame cap with MAP only (read-only, no WRITE/EXECUTE).
            ResourceType::PlatformTable =>
            {
                let obj = Box::new(FrameObject {
                    header: KernelObjectHeader::new(ObjectType::Frame),
                    base: res.base,
                    size: res.size,
                });
                let ptr = nonnull_from_box(obj);
                insert_or_fatal(
                    cspace,
                    CapTag::Frame,
                    Rights::MAP,
                    ptr,
                    "Phase 7: cannot allocate Frame capability for firmware table",
                );
            }

            // Interrupt lines: Interrupt cap with SIGNAL right so drivers can
            // call SYS_IRQ_REGISTER and SYS_IRQ_ACK.
            ResourceType::IrqLine =>
            {
                let obj = Box::new(InterruptObject {
                    header: KernelObjectHeader::new(ObjectType::Interrupt),
                    irq_id: res.id as u32,
                    flags: res.flags,
                });
                let ptr = nonnull_from_box(obj);
                insert_or_fatal(
                    cspace,
                    CapTag::Interrupt,
                    Rights::SIGNAL,
                    ptr,
                    "Phase 7: cannot allocate Interrupt capability",
                );
            }

            // I/O port ranges (x86-64 only — validated/filtered in Phase 6).
            ResourceType::IoPortRange =>
            {
                let obj = Box::new(IoPortRangeObject {
                    header: KernelObjectHeader::new(ObjectType::IoPortRange),
                    base: res.base as u16,
                    size: res.size as u16,
                    _pad: 0,
                });
                let ptr = nonnull_from_box(obj);
                insert_or_fatal(
                    cspace,
                    CapTag::IoPortRange,
                    Rights::USE,
                    ptr,
                    "Phase 7: cannot allocate IoPortRange capability",
                );
            }
        }
    }

    // One SchedControl capability — grants elevated scheduling authority.
    let obj = Box::new(SchedControlObject {
        header: KernelObjectHeader::new(ObjectType::SchedControl),
    });
    let ptr = nonnull_from_box(obj);
    insert_or_fatal(
        cspace,
        CapTag::SchedControl,
        Rights::ELEVATE,
        ptr,
        "Phase 7: cannot allocate SchedControl capability",
    );
}

/// Cast `Box<T>` to `NonNull<KernelObjectHeader>` by leaking the box.
///
/// # Safety contract
///
/// `T` must be `#[repr(C)]` with `KernelObjectHeader` as its first field
/// (offset 0). Dropping the returned pointer requires reconstructing the
/// original `Box<T>` based on `header.obj_type` (future phases).
fn nonnull_from_box<T>(b: Box<T>) -> NonNull<KernelObjectHeader>
{
    let raw = Box::into_raw(b).cast::<KernelObjectHeader>();
    // SAFETY: Box::into_raw never returns null.
    unsafe { NonNull::new_unchecked(raw) }
}

/// Move a capability between `CSpaces`, rewriting derivation tree pointers in place.
///
/// The moved slot takes the **same position** in the derivation tree as the source:
/// parent, children, and siblings all have their pointers updated to the new
/// `(dst_cspace_id, new_idx)` location. No ref-count change occurs — this is a
/// move, not a copy.
///
/// # Contract
/// - **Caller must hold [`DERIVATION_LOCK`]`.write_lock()`** for the duration.
/// - Source slot must be non-null.
/// - `dst_cspace` must have at least one free slot (call `pre_allocate` first).
///
/// Returns the new slot index in `dst_cspace`, or an error if the source slot
/// is null/invalid or the destination `CSpace` is full.
///
/// # Safety
/// `src_cspace` and `dst_cspace` must be valid, live `CSpace` pointers.
///
/// # To add support for explicit destination index
/// Add a `dst_idx: Option<u32>` parameter and call `insert_cap_at` when `Some`.
#[cfg(not(test))]
pub unsafe fn move_cap_between_cspaces(
    src_cspace: *mut CSpace,
    src_idx: u32,
    dst_cspace: *mut CSpace,
) -> Result<u32, syscall::SyscallError>
{
    use syscall::SyscallError;

    // Read source slot (tag, rights, object pointer).
    let (src_tag, src_rights, src_object) = {
        let cs = unsafe { &*src_cspace };
        let slot = cs.slot(src_idx).ok_or(SyscallError::InvalidCapability)?;
        if slot.tag == CapTag::Null
        {
            return Err(SyscallError::InvalidCapability);
        }
        (
            slot.tag,
            slot.rights,
            slot.object.ok_or(SyscallError::InvalidCapability)?,
        )
    };

    let src_cspace_id = unsafe { (*src_cspace).id() };
    let dst_cspace_id = unsafe { (*dst_cspace).id() };

    // Insert into destination (auto-allocate free slot).
    let new_idx = unsafe { (*dst_cspace).insert_cap(src_tag, src_rights, src_object) }
        .map_err(|_| SyscallError::OutOfMemory)?;

    let src_slot_id = SlotId::new(src_cspace_id, src_idx);
    let dst_slot_id = SlotId::new(dst_cspace_id, new_idx);

    // Read derivation links from the source slot.
    let (src_parent, src_first_child, src_prev, src_next) = {
        let cs = unsafe { &*src_cspace };
        let slot = cs.slot(src_idx).unwrap();
        (
            slot.deriv_parent,
            slot.deriv_first_child,
            slot.deriv_prev_sibling,
            slot.deriv_next_sibling,
        )
    };

    // Copy derivation links to the destination slot.
    if let Some(dst_slot) = unsafe { (*dst_cspace).slot_mut(new_idx) }
    {
        dst_slot.deriv_parent = src_parent;
        dst_slot.deriv_first_child = src_first_child;
        dst_slot.deriv_prev_sibling = src_prev;
        dst_slot.deriv_next_sibling = src_next;
    }

    // Update parent's first_child if it pointed to source.
    if let Some(parent_id) = src_parent
    {
        if let Some(parent_cs) = lookup_cspace(parent_id.cspace_id)
        {
            if let Some(parent_slot) = unsafe { (*parent_cs).slot_mut(parent_id.index.get()) }
            {
                if parent_slot.deriv_first_child == Some(src_slot_id)
                {
                    parent_slot.deriv_first_child = Some(dst_slot_id);
                }
            }
        }
    }

    // Update prev sibling's next pointer.
    if let Some(prev_id) = src_prev
    {
        if let Some(prev_cs) = lookup_cspace(prev_id.cspace_id)
        {
            if let Some(prev_slot) = unsafe { (*prev_cs).slot_mut(prev_id.index.get()) }
            {
                if prev_slot.deriv_next_sibling == Some(src_slot_id)
                {
                    prev_slot.deriv_next_sibling = Some(dst_slot_id);
                }
            }
        }
    }

    // Update next sibling's prev pointer.
    if let Some(next_id) = src_next
    {
        if let Some(next_cs) = lookup_cspace(next_id.cspace_id)
        {
            if let Some(next_slot) = unsafe { (*next_cs).slot_mut(next_id.index.get()) }
            {
                if next_slot.deriv_prev_sibling == Some(src_slot_id)
                {
                    next_slot.deriv_prev_sibling = Some(dst_slot_id);
                }
            }
        }
    }

    // Update all children's parent pointer.
    // Walk via next_sibling; children's order is preserved.
    let mut child_cur = src_first_child;
    while let Some(child_id) = child_cur
    {
        child_cur = if let Some(child_cs) = lookup_cspace(child_id.cspace_id)
        {
            if let Some(child_slot) = unsafe { (*child_cs).slot_mut(child_id.index.get()) }
            {
                child_slot.deriv_parent = Some(dst_slot_id);
                child_slot.deriv_next_sibling
            }
            else
            {
                None
            }
        }
        else
        {
            None
        };
    }

    // Clear the source slot. No inc_ref/dec_ref — it's a move.
    unsafe {
        (*src_cspace).free_slot(src_idx);
    }

    Ok(new_idx)
}

/// Insert a capability, calling [`crate::fatal`] on error.
fn insert_or_fatal(
    cspace: &mut CSpace,
    tag: CapTag,
    rights: Rights,
    object: NonNull<KernelObjectHeader>,
    msg: &'static str,
) -> u32
{
    match cspace.insert_cap(tag, rights, object)
    {
        Ok(idx) => idx,
        #[cfg(not(test))]
        Err(_) => crate::fatal(msg),
        // In test mode, panic with the message instead of halting the CPU.
        #[cfg(test)]
        Err(e) => panic!("{}: {:?}", msg, e),
    }
}
