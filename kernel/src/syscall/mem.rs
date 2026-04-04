// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/syscall/mem.rs

//! Memory management syscall handlers.
//!
//! # Adding new memory syscalls
//! 1. Add a new `pub fn sys_mem_*` in this file.
//! 2. Add the syscall constant import to `syscall/mod.rs`.
//! 3. Add a dispatch arm to `syscall/mod.rs`.
//! 4. Add a userspace wrapper to `shared/syscall/src/lib.rs`.

use crate::arch::current::trap_frame::TrapFrame;
use syscall::SyscallError;

/// SYS_MEM_MAP (16): map a physical Frame into a user address space.
///
/// arg0 = Frame cap index (must have MAP right; WRITE/EXECUTE determine page perms).
/// arg1 = AddressSpace cap index (must have MAP right).
/// arg2 = virtual address of the first page to map (must be page-aligned, user range).
/// arg3 = offset into the frame in pages (0 = start of frame).
/// arg4 = number of pages to map.
///
/// Page flags are derived from the Frame cap's rights:
/// - `WRITE` set → writable (exclusive with EXECUTE, W^X enforced).
/// - `EXECUTE` set → executable (exclusive with WRITE).
/// - Neither → read-only.
///
/// Returns 0 on success.
#[cfg(not(test))]
pub fn sys_mem_map(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    use crate::cap::object::{AddressSpaceObject, FrameObject};
    use crate::cap::slot::{CapTag, Rights};
    use crate::mm::paging::PageFlags;
    use crate::mm::{with_frame_allocator, PAGE_SIZE};
    use crate::syscall::current_tcb;

    let frame_idx = tf.arg(0) as u32;
    let aspace_idx = tf.arg(1) as u32;
    let virt_base = tf.arg(2);
    let offset_pages = tf.arg(3) as usize;
    let page_count = tf.arg(4) as usize;

    // ── Validation ────────────────────────────────────────────────────────────

    // Virtual address must be page-aligned.
    if virt_base & 0xFFF != 0
    {
        return Err(SyscallError::InvalidAddress);
    }

    // Virtual address must be in the user half (< canonical kernel boundary).
    const USER_HALF_TOP: u64 = 0x0000_8000_0000_0000;
    if virt_base >= USER_HALF_TOP
    {
        return Err(SyscallError::InvalidAddress);
    }

    // Reject zero-length mappings.
    if page_count == 0
    {
        return Err(SyscallError::InvalidArgument);
    }

    // Guard against overflow in the virtual range.
    let mapping_size = page_count
        .checked_mul(PAGE_SIZE)
        .ok_or(SyscallError::InvalidArgument)?;
    let virt_end = virt_base.checked_add(mapping_size as u64).ok_or(SyscallError::InvalidArgument)?;
    if virt_end > USER_HALF_TOP
    {
        return Err(SyscallError::InvalidAddress);
    }

    // ── Capability lookup ─────────────────────────────────────────────────────

    let tcb = unsafe { current_tcb() };
    if tcb.is_null()
    {
        return Err(SyscallError::InvalidCapability);
    }
    let caller_cspace = unsafe { (*tcb).cspace };
    if caller_cspace.is_null()
    {
        return Err(SyscallError::InvalidCapability);
    }

    // Resolve Frame cap.
    let frame_slot = unsafe { super::lookup_cap(caller_cspace, frame_idx, CapTag::Frame, Rights::MAP) }?;
    let (frame_phys, frame_size) = {
        let obj = frame_slot.object.ok_or(SyscallError::InvalidCapability)?;
        // SAFETY: tag confirmed Frame; pointer is valid.
        let fo = unsafe { &*(obj.as_ptr() as *const FrameObject) };
        (fo.base, fo.size)
    };
    let frame_rights = frame_slot.rights;

    // Validate that offset + page_count stays within the frame.
    let byte_offset = offset_pages
        .checked_mul(PAGE_SIZE)
        .ok_or(SyscallError::InvalidArgument)? as u64;
    let byte_end = byte_offset
        .checked_add(mapping_size as u64)
        .ok_or(SyscallError::InvalidArgument)?;
    if byte_end > frame_size
    {
        return Err(SyscallError::InvalidArgument);
    }

    // Derive page permission flags from the Frame cap's rights.
    let writable = frame_rights.contains(Rights::WRITE);
    let executable = frame_rights.contains(Rights::EXECUTE);
    // W^X is enforced at cap creation time, but double-check defensively.
    if writable && executable
    {
        return Err(SyscallError::WxViolation);
    }
    let page_flags = PageFlags {
        readable:   true,
        writable,
        executable,
        uncacheable: false,
    };

    // Resolve AddressSpace cap.
    let aspace_slot = unsafe {
        super::lookup_cap(caller_cspace, aspace_idx, CapTag::AddressSpace, Rights::MAP)
    }?;
    let as_ptr = {
        let obj = aspace_slot.object.ok_or(SyscallError::InvalidCapability)?;
        // SAFETY: tag confirmed AddressSpace; pointer is valid.
        let as_obj = unsafe { &*(obj.as_ptr() as *const AddressSpaceObject) };
        as_obj.address_space
    };

    // ── Mapping loop ──────────────────────────────────────────────────────────

    for i in 0..page_count
    {
        let virt = virt_base + (i * PAGE_SIZE) as u64;
        let phys = frame_phys + byte_offset + (i * PAGE_SIZE) as u64;

        let result = with_frame_allocator(|alloc| {
            // SAFETY: virt is in user range (validated above); phys is from a
            // Frame cap confirmed by the kernel at capability creation.
            unsafe { (*as_ptr).map_page(virt, phys, page_flags, alloc) }
        });

        result.map_err(|_| SyscallError::OutOfMemory)?;
    }

    Ok(0)
}

// Test stub.
#[cfg(test)]
pub fn sys_mem_map(_tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    Err(SyscallError::NotSupported)
}

// ── SYS_MEM_UNMAP ─────────────────────────────────────────────────────────────

/// SYS_MEM_UNMAP (17): remove page mappings from a user address space.
///
/// arg0 = AddressSpace cap index (must have MAP right).
/// arg1 = virtual address of the first page to unmap (page-aligned, user range).
/// arg2 = number of pages to unmap (non-zero).
///
/// Unmapping a page that is not mapped is a no-op (not an error).
/// Returns 0 on success.
///
/// Note: intermediate page table frames are not reclaimed; full teardown
/// happens when the address space object is destroyed.
#[cfg(not(test))]
pub fn sys_mem_unmap(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    use crate::cap::object::AddressSpaceObject;
    use crate::cap::slot::{CapTag, Rights};
    use crate::mm::PAGE_SIZE;
    use crate::syscall::current_tcb;

    let aspace_idx = tf.arg(0) as u32;
    let virt_base  = tf.arg(1);
    let page_count = tf.arg(2) as usize;

    // ── Validation ────────────────────────────────────────────────────────────

    if virt_base & 0xFFF != 0
    {
        return Err(SyscallError::InvalidAddress);
    }
    const USER_HALF_TOP: u64 = 0x0000_8000_0000_0000;
    if virt_base >= USER_HALF_TOP
    {
        return Err(SyscallError::InvalidAddress);
    }
    if page_count == 0
    {
        return Err(SyscallError::InvalidArgument);
    }
    let mapping_size = page_count.checked_mul(PAGE_SIZE).ok_or(SyscallError::InvalidArgument)?;
    let virt_end = virt_base.checked_add(mapping_size as u64).ok_or(SyscallError::InvalidArgument)?;
    if virt_end > USER_HALF_TOP
    {
        return Err(SyscallError::InvalidAddress);
    }

    // ── Capability lookup ─────────────────────────────────────────────────────

    let tcb = unsafe { current_tcb() };
    if tcb.is_null() { return Err(SyscallError::InvalidCapability); }
    let caller_cspace = unsafe { (*tcb).cspace };
    if caller_cspace.is_null() { return Err(SyscallError::InvalidCapability); }

    let aspace_slot = unsafe {
        super::lookup_cap(caller_cspace, aspace_idx, CapTag::AddressSpace, Rights::MAP)
    }?;
    let as_ptr = {
        let obj = aspace_slot.object.ok_or(SyscallError::InvalidCapability)?;
        // SAFETY: tag confirmed AddressSpace.
        let as_obj = unsafe { &*(obj.as_ptr() as *const AddressSpaceObject) };
        as_obj.address_space
    };

    // ── Unmap loop ────────────────────────────────────────────────────────────

    for i in 0..page_count
    {
        let virt = virt_base + (i * PAGE_SIZE) as u64;
        // SAFETY: virt is in user range (validated above); as_ptr is valid.
        unsafe { (*as_ptr).unmap_page(virt) };
    }

    Ok(0)
}

// Test stub.
#[cfg(test)]
pub fn sys_mem_unmap(_tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    Err(SyscallError::NotSupported)
}

// ── SYS_MEM_PROTECT ───────────────────────────────────────────────────────────

/// SYS_MEM_PROTECT (18): change permission flags on existing page mappings.
///
/// arg0 = Frame cap index (must have MAP right; authorises the new permissions).
/// arg1 = AddressSpace cap index (must have MAP right).
/// arg2 = virtual address of the first page (page-aligned, user range).
/// arg3 = number of pages (non-zero).
/// arg4 = new protection bits: bit 1 = WRITE, bit 2 = EXECUTE (matches Rights layout).
///
/// The new permissions must be a subset of the Frame cap's rights. W^X is
/// enforced: WRITE and EXECUTE may not both be set. Protecting a page that
/// is not mapped returns `InvalidAddress`.
///
/// Returns 0 on success.
#[cfg(not(test))]
pub fn sys_mem_protect(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    use crate::cap::object::AddressSpaceObject;
    use crate::cap::slot::{CapTag, Rights};
    use crate::mm::paging::{PageFlags, PagingError};
    use crate::mm::PAGE_SIZE;
    use crate::syscall::current_tcb;

    let frame_idx  = tf.arg(0) as u32;
    let aspace_idx = tf.arg(1) as u32;
    let virt_base  = tf.arg(2);
    let page_count = tf.arg(3) as usize;
    let prot_bits  = tf.arg(4);

    // ── Validation ────────────────────────────────────────────────────────────

    if virt_base & 0xFFF != 0
    {
        return Err(SyscallError::InvalidAddress);
    }
    const USER_HALF_TOP: u64 = 0x0000_8000_0000_0000;
    if virt_base >= USER_HALF_TOP
    {
        return Err(SyscallError::InvalidAddress);
    }
    if page_count == 0
    {
        return Err(SyscallError::InvalidArgument);
    }
    let mapping_size = page_count.checked_mul(PAGE_SIZE).ok_or(SyscallError::InvalidArgument)?;
    let virt_end = virt_base.checked_add(mapping_size as u64).ok_or(SyscallError::InvalidArgument)?;
    if virt_end > USER_HALF_TOP
    {
        return Err(SyscallError::InvalidAddress);
    }

    // Parse new protection bits (bit 1 = WRITE, bit 2 = EXECUTE per Rights layout).
    let writable   = (prot_bits & 0x2) != 0;
    let executable = (prot_bits & 0x4) != 0;

    // W^X check.
    if writable && executable
    {
        return Err(SyscallError::WxViolation);
    }

    // ── Capability lookup ─────────────────────────────────────────────────────

    let tcb = unsafe { current_tcb() };
    if tcb.is_null() { return Err(SyscallError::InvalidCapability); }
    let caller_cspace = unsafe { (*tcb).cspace };
    if caller_cspace.is_null() { return Err(SyscallError::InvalidCapability); }

    // Frame cap authorises the permission level.
    let frame_slot = unsafe {
        super::lookup_cap(caller_cspace, frame_idx, CapTag::Frame, Rights::MAP)
    }?;
    // Verify object pointer is valid; rights are read from the slot directly.
    let _ = frame_slot.object.ok_or(SyscallError::InvalidCapability)?;
    let frame_rights = frame_slot.rights;

    // Requested permissions must be a subset of what the Frame cap allows.
    if writable && !frame_rights.contains(Rights::WRITE)
    {
        return Err(SyscallError::InsufficientRights);
    }
    if executable && !frame_rights.contains(Rights::EXECUTE)
    {
        return Err(SyscallError::InsufficientRights);
    }

    let page_flags = PageFlags {
        readable: true,
        writable,
        executable,
        uncacheable: false,
    };

    let aspace_slot = unsafe {
        super::lookup_cap(caller_cspace, aspace_idx, CapTag::AddressSpace, Rights::MAP)
    }?;
    let as_ptr = {
        let obj = aspace_slot.object.ok_or(SyscallError::InvalidCapability)?;
        // SAFETY: tag confirmed AddressSpace.
        let as_obj = unsafe { &*(obj.as_ptr() as *const AddressSpaceObject) };
        as_obj.address_space
    };

    // ── Protect loop ──────────────────────────────────────────────────────────

    for i in 0..page_count
    {
        let virt = virt_base + (i * PAGE_SIZE) as u64;
        // SAFETY: virt is in user range (validated above); as_ptr is valid.
        unsafe { (*as_ptr).protect_page(virt, page_flags) }
            .map_err(|e| match e
            {
                PagingError::NotMapped => SyscallError::InvalidAddress,
                _ => SyscallError::InvalidArgument,
            })?;
    }

    Ok(0)
}

// Test stub.
#[cfg(test)]
pub fn sys_mem_protect(_tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    Err(SyscallError::NotSupported)
}

// ── SYS_FRAME_SPLIT ───────────────────────────────────────────────────────────

/// SYS_FRAME_SPLIT (33): split a Frame cap into two non-overlapping children.
///
/// arg0 = Frame cap index (must have MAP right).
/// arg1 = split offset in bytes (page-aligned; must be > 0 and < frame size).
/// arg2 = reserved (must be 0).
///
/// Consumes the original cap and creates two new Frame caps with the same
/// rights, covering `[base, base+split_offset)` and `[base+split_offset, end)`.
/// Both children are reparented to the original cap's derivation parent (same
/// revocability semantics as the sibling caps).
///
/// Returns `slot1 | (slot2 << 32)` on success.
#[cfg(not(test))]
pub fn sys_frame_split(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    extern crate alloc;
    use alloc::boxed::Box;
    use core::ptr::NonNull;

    use crate::cap::derivation::{link_child, reparent_children, unlink_node, DERIVATION_LOCK};
    use crate::cap::object::{dealloc_object, FrameObject, KernelObjectHeader, ObjectType};
    use crate::cap::slot::{CapTag, Rights, SlotId};
    use crate::mm::PAGE_SIZE;
    use crate::syscall::current_tcb;

    let frame_idx    = tf.arg(0) as u32;
    let split_offset = tf.arg(1);
    // arg2 is reserved; ignore.

    // ── Validation ────────────────────────────────────────────────────────────

    if split_offset & 0xFFF != 0
    {
        return Err(SyscallError::InvalidArgument); // must be page-aligned
    }
    if split_offset == 0
    {
        return Err(SyscallError::InvalidArgument);
    }

    // ── Capability lookup ─────────────────────────────────────────────────────

    let tcb = unsafe { current_tcb() };
    if tcb.is_null() { return Err(SyscallError::InvalidCapability); }
    let caller_cspace = unsafe { (*tcb).cspace };
    if caller_cspace.is_null() { return Err(SyscallError::InvalidCapability); }

    let (frame_phys, frame_size, frame_rights, cspace_id, orig_obj_ptr) = {
        let slot = unsafe {
            super::lookup_cap(caller_cspace, frame_idx, CapTag::Frame, Rights::MAP)
        }?;
        let obj_ptr = slot.object.ok_or(SyscallError::InvalidCapability)?;
        let fo = unsafe { &*(obj_ptr.as_ptr() as *const FrameObject) };
        let cspace_id = unsafe { (*caller_cspace).id() };
        (fo.base, fo.size, slot.rights, cspace_id, obj_ptr)
    };

    // split_offset must be strictly within [1, frame_size).
    if split_offset >= frame_size
    {
        return Err(SyscallError::InvalidArgument);
    }
    // At least one page must remain on each side.
    if frame_size - split_offset < PAGE_SIZE as u64
    {
        return Err(SyscallError::InvalidArgument);
    }

    // ── Create two child FrameObjects ─────────────────────────────────────────

    // Allocate child1: [base, base + split_offset).
    let child1_obj = Box::new(FrameObject {
        header: crate::cap::object::KernelObjectHeader::new(ObjectType::Frame),
        base:   frame_phys,
        size:   split_offset,
    });
    let child1_ptr: NonNull<KernelObjectHeader> = {
        let raw = Box::into_raw(child1_obj) as *mut KernelObjectHeader;
        // SAFETY: Box::into_raw is non-null.
        unsafe { NonNull::new_unchecked(raw) }
    };

    // Allocate child2: [base + split_offset, end).
    let child2_obj = Box::new(FrameObject {
        header: crate::cap::object::KernelObjectHeader::new(ObjectType::Frame),
        base:   frame_phys + split_offset,
        size:   frame_size - split_offset,
    });
    let child2_ptr: NonNull<KernelObjectHeader> = {
        let raw = Box::into_raw(child2_obj) as *mut KernelObjectHeader;
        unsafe { NonNull::new_unchecked(raw) }
    };

    // Insert both children into the caller's CSpace (auto-allocate slots).
    let cs = unsafe { &mut *caller_cspace };
    let slot1 = cs.insert_cap(CapTag::Frame, frame_rights, child1_ptr)
        .map_err(|_| SyscallError::OutOfMemory)?;
    let slot2 = cs.insert_cap(CapTag::Frame, frame_rights, child2_ptr)
        .map_err(|_| {
            // Undo slot1 insertion on failure.
            cs.free_slot(slot1);
            unsafe { crate::cap::object::dealloc_object(child1_ptr) };
            SyscallError::OutOfMemory
        })?;

    // ── Wire derivation tree ──────────────────────────────────────────────────
    //
    // Pattern mirrors sys_cap_delete: reparent original's children to its
    // parent, unlink original, then link both new caps to that same parent.

    DERIVATION_LOCK.write_lock();

    let orig_node  = SlotId::new(cspace_id, frame_idx);
    let child1_id  = SlotId::new(cspace_id, slot1);
    let child2_id  = SlotId::new(cspace_id, slot2);

    // Read the original's parent before we modify anything.
    let orig_parent = unsafe {
        (*caller_cspace).slot(frame_idx).and_then(|s| s.deriv_parent)
    };

    // Reparent original's existing children (if any) to its parent.
    unsafe { reparent_children(orig_node, orig_parent) };
    // Unlink the original node from the tree.
    unsafe { unlink_node(orig_node) };

    // Link both new caps to the original's parent (if any).
    if let Some(parent_id) = orig_parent
    {
        unsafe { link_child(parent_id, child1_id) };
        unsafe { link_child(parent_id, child2_id) };
    }

    DERIVATION_LOCK.write_unlock();

    // ── Consume the original cap ──────────────────────────────────────────────

    // Return original slot to free list (tag becomes Null).
    unsafe { (*caller_cspace).free_slot(frame_idx) };

    // Dec-ref original object; free if no references remain.
    let remaining = unsafe { (*orig_obj_ptr.as_ptr()).dec_ref() };
    if remaining == 0
    {
        unsafe { dealloc_object(orig_obj_ptr) };
    }

    Ok((slot1 as u64) | ((slot2 as u64) << 32))
}

// Test stub.
#[cfg(test)]
pub fn sys_frame_split(_tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    Err(SyscallError::NotSupported)
}
