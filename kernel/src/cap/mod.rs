// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/cap/mod.rs

//! Capability subsystem (Phase 7).
//!
//! Initialised by [`init_capability_system`], which creates the root CSpace
//! (id 0) populated with initial capabilities for all boot-provided hardware
//! resources:
//!
//! - Usable physical memory → [`CapTag::Frame`] caps (MAP | WRITE)
//! - MMIO ranges (MmioRange, PciEcam, IommuUnit) → [`CapTag::MmioRegion`] caps (MAP)
//! - Interrupt lines → [`CapTag::Interrupt`] caps
//! - Firmware tables → [`CapTag::Frame`] caps (MAP only, no WRITE)
//! - I/O port ranges (x86-64) → [`CapTag::IoPortRange`] caps (USE)
//! - One [`CapTag::SchedControl`] cap (ELEVATE)
//!
//! The populated CSpace is stored in [`ROOT_CSPACE`] until Phase 9 hands it
//! to the init process.

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;

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
    FrameObject, InterruptObject, IoPortRangeObject, KernelObjectHeader, MmioRegionObject,
    ObjectType, SchedControlObject,
};
#[allow(unused_imports)]
pub use slot::{CSpaceId, CapTag, CapabilitySlot, Rights, SlotId};

use boot_protocol::{BootInfo, MemoryMapEntry, MemoryType, PlatformResource, ResourceType};
use core::ptr::NonNull;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::mm::paging::phys_to_virt;

// ── Globals ───────────────────────────────────────────────────────────────────

/// Root capability space, populated during Phase 7.
///
/// Consumed (transferred to init) during Phase 9. Access is single-threaded
/// during boot; `static mut` is safe here under that invariant.
#[cfg(not(test))]
#[allow(static_mut_refs)]
pub static mut ROOT_CSPACE: Option<Box<CSpace>> = None;

/// Monotonically increasing CSpace ID allocator. Root gets ID 0.
static NEXT_CSPACE_ID: AtomicU32 = AtomicU32::new(0);

/// Maximum slots in the root CSpace (full two-level directory).
const ROOT_CSPACE_MAX_SLOTS: usize = 16384;

// ── Phase 7 entry point ───────────────────────────────────────────────────────

/// Initialise the capability system and populate the root CSpace.
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
    platform_resources: Vec<PlatformResource>,
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

    populate_cspace(&mut cspace, mmap, &platform_resources);

    let count = cspace.populated_count();

    // Store in ROOT_CSPACE (kernel runtime only; test builds skip this).
    #[cfg(not(test))]
    // SAFETY: single-threaded boot; ROOT_CSPACE not yet accessed.
    unsafe {
        ROOT_CSPACE = Some(cspace);
    }
    // In test mode the box is dropped here — kernel objects are leaked
    // intentionally via Box::into_raw in nonnull_from_box, which is
    // acceptable for isolated unit tests.
    #[cfg(test)]
    let _ = cspace;

    count
}

/// Core CSpace population logic, separated for testability.
///
/// Creates one capability per usable memory region, per platform resource,
/// and one SchedControl capability.
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

            // Interrupt lines: Interrupt cap (no specific right bits defined).
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
                    Rights::NONE,
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
    let raw = Box::into_raw(b) as *mut KernelObjectHeader;
    // SAFETY: Box::into_raw never returns null.
    unsafe { NonNull::new_unchecked(raw) }
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
