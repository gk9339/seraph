// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/cap/cspace.rs

//! Capability space implementation.
//!
//! A [`CSpace`] is a two-level directory of [`CapabilitySlot`]s. The directory
//! has [`L1_SIZE`] entries; each points to a [`CSpacePage`] containing
//! [`L2_SIZE`] slots. Maximum capacity: `L1_SIZE * L2_SIZE = 16384` slots.
//!
//! ## Free list
//!
//! Freed slots are tracked via an intrusive singly-linked list encoded in each
//! slot's `deriv_parent` field (see `slot.rs`). Slot 0 is permanently null and
//! is never placed on the free list.
//!
//! ## Growth
//!
//! CSpace pages are allocated on demand by [`CSpace::grow`]. The first page
//! skips slot 0 (always null); subsequent pages contribute all 64 slots to the
//! free list.

// In no_std builds alloc must be declared explicitly; std builds include it implicitly.
extern crate alloc;

use alloc::boxed::Box;
use core::ptr::NonNull;

use super::object::KernelObjectHeader;
use super::slot::{violates_wx, CSpaceId, CapTag, CapabilitySlot, Rights};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Slots per CSpace page (64 × 48 B = 3072 B, fits in a 4096-byte slab bin).
pub const L2_SIZE: usize = 64;

/// Directory entries per CSpace (max 256 × 64 = 16384 slots).
pub const L1_SIZE: usize = 256;

// ── Error type ────────────────────────────────────────────────────────────────

/// Errors returned by CSpace operations.
#[derive(Debug, PartialEq, Eq)]
pub enum CapError
{
    /// No free slots remain and the CSpace is at `max_slots`.
    OutOfSlots,
    /// Heap allocation failed while growing the CSpace.
    OutOfMemory,
    /// The provided slot index is out of range or unmapped.
    InvalidIndex,
    /// Rights bitmask violates the W^X constraint.
    WxViolation,
}

// ── CSpacePage ────────────────────────────────────────────────────────────────

/// One page of capability slots.
///
/// Allocated as a `Box<CSpacePage>` when the CSpace grows. All-zeros is a
/// valid initial state (every slot is null), so pages are allocated via
/// `unsafe { core::mem::zeroed() }`.
#[repr(C)]
struct CSpacePage
{
    slots: [CapabilitySlot; L2_SIZE],
}

// ── CSpace ────────────────────────────────────────────────────────────────────

/// A capability space: a growable indexed collection of capability slots.
///
/// Slots are identified by a `u32` index. Slot 0 is permanently null. Indices
/// are stable for the lifetime of the capability they hold.
///
/// To add a capability: call [`insert_cap`][CSpace::insert_cap].
/// To look up a slot: call [`slot`][CSpace::slot] or [`slot_mut`][CSpace::slot_mut].
pub struct CSpace
{
    id: CSpaceId,
    /// Two-level directory; each Some entry is a 64-slot page.
    directory: [Option<Box<CSpacePage>>; L1_SIZE],
    /// Total usable slots allocated across all pages (excludes slot 0).
    allocated_slots: usize,
    /// Maximum number of usable slots this CSpace may hold.
    max_slots: usize,
    /// Head of the intrusive free list; None if no free slots.
    free_head: Option<u32>,
    /// Number of slots currently on the free list (for O(1) pre_allocate).
    free_count: usize,
}

impl CSpace
{
    /// Create an empty CSpace. No pages are allocated until the first slot
    /// is requested.
    pub fn new(id: CSpaceId, max_slots: usize) -> Self
    {
        Self {
            id,
            directory: core::array::from_fn(|_| None),
            allocated_slots: 0,
            max_slots,
            free_head: None,
            free_count: 0,
        }
    }

    /// Return this CSpace's unique identifier.
    pub fn id(&self) -> CSpaceId
    {
        self.id
    }

    /// Allocate a free slot index, growing the CSpace if needed.
    ///
    /// Returns an error if `max_slots` is reached or heap allocation fails.
    /// The returned slot is cleared to null; callers must populate it.
    pub fn allocate_slot(&mut self) -> Result<u32, CapError>
    {
        if self.free_head.is_none()
        {
            self.grow()?;
        }

        let idx = self.free_head.ok_or(CapError::OutOfSlots)?;

        // Read next_free through a shared borrow, then drop it before the
        // mutable borrow so the borrow checker is satisfied.
        let next = {
            let slot = self.slot(idx).ok_or(CapError::InvalidIndex)?;
            slot.next_free()
        };

        self.free_head = next;
        // Clear the slot (removes free-list encoding).
        self.slot_mut(idx).unwrap().clear();
        self.free_count -= 1;
        Ok(idx)
    }

    /// Grow the CSpace by one page.
    ///
    /// Allocates the next unoccupied directory entry, threads all its slots
    /// onto the free list, then returns. Slot 0 in the first page is skipped.
    fn grow(&mut self) -> Result<(), CapError>
    {
        let page_idx = self
            .directory
            .iter()
            .position(|p: &Option<Box<CSpacePage>>| p.is_none())
            .ok_or(CapError::OutOfSlots)?;

        let base = page_idx * L2_SIZE;
        // Skip slot 0 in the first page (permanently null, not in free list).
        let start_slot = if page_idx == 0 { 1usize } else { 0usize };
        let new_free = L2_SIZE - start_slot;

        // Enforce the configured max_slots ceiling.
        if self.allocated_slots + new_free > self.max_slots
        {
            return Err(CapError::OutOfSlots);
        }

        // Allocate page (all-zeros = all null slots).
        // SAFETY: all-zeros is a valid CSpacePage: every CapabilitySlot is null
        // (Null tag = 0, Rights::NONE = 0, NonNull/Option niches encode None at 0).
        let mut page = Box::new(unsafe { core::mem::zeroed::<CSpacePage>() });

        // Thread slots onto the free list in reverse order so ascending indices
        // are popped in ascending order (not required for correctness but nice).
        let old_head = self.free_head;
        let mut next = old_head;
        for i in (start_slot..L2_SIZE).rev()
        {
            let idx = (base + i) as u32;
            page.slots[i].set_next_free(next);
            next = Some(idx);
        }
        self.free_head = next;

        self.allocated_slots += new_free;
        self.free_count += new_free;
        self.directory[page_idx] = Some(page);
        Ok(())
    }

    /// Look up a slot by index. Returns `None` if the index is out of range
    /// or the backing page has not been allocated.
    pub fn slot(&self, index: u32) -> Option<&CapabilitySlot>
    {
        let idx = index as usize;
        let page_idx = idx / L2_SIZE;
        let slot_idx = idx % L2_SIZE;
        self.directory[page_idx]
            .as_ref()
            .map(|p| &p.slots[slot_idx])
    }

    /// Mutable variant of [`slot`][Self::slot].
    pub fn slot_mut(&mut self, index: u32) -> Option<&mut CapabilitySlot>
    {
        let idx = index as usize;
        let page_idx = idx / L2_SIZE;
        let slot_idx = idx % L2_SIZE;
        self.directory[page_idx]
            .as_mut()
            .map(|p| &mut p.slots[slot_idx])
    }

    /// Return a slot to the free list and clear its contents.
    ///
    /// Silently ignores an out-of-range or unmapped index.
    pub fn free_slot(&mut self, index: u32)
    {
        let old_head = self.free_head;
        if let Some(slot) = self.slot_mut(index)
        {
            slot.set_next_free(old_head);
            self.free_head = Some(index);
            self.free_count += 1;
        }
    }

    /// Allocate a slot, populate it with the given capability, and return the
    /// slot index.
    ///
    /// Returns [`CapError::WxViolation`] if `rights` has both WRITE and EXECUTE.
    pub fn insert_cap(
        &mut self,
        tag: CapTag,
        rights: Rights,
        object: NonNull<KernelObjectHeader>,
    ) -> Result<u32, CapError>
    {
        if violates_wx(rights)
        {
            return Err(CapError::WxViolation);
        }

        let index = self.allocate_slot()?;

        // SAFETY: allocate_slot returned a valid index into an allocated page.
        let slot = self.slot_mut(index).ok_or(CapError::InvalidIndex)?;
        slot.tag = tag;
        slot.rights = rights;
        slot.object = Some(object);
        slot.deriv_parent = None;
        slot.deriv_first_child = None;
        slot.deriv_next_sibling = None;
        slot.deriv_prev_sibling = None;

        Ok(index)
    }

    /// Grow the CSpace until at least `min_free` slots are available without
    /// a further grow. Used to pre-warm the free list before bulk insertions.
    pub fn pre_allocate(&mut self, min_free: usize) -> Result<(), CapError>
    {
        while self.free_count < min_free
        {
            self.grow()?;
        }
        Ok(())
    }

    /// Count the number of non-null (occupied) slots.
    ///
    /// O(1): derived from `allocated_slots - free_count`.
    pub fn populated_count(&self) -> usize
    {
        self.allocated_slots - self.free_count
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests
{
    use super::*;
    use crate::cap::object::{FrameObject, KernelObjectHeader, ObjectType};
    use core::ptr::NonNull;

    /// Construct a dummy NonNull<KernelObjectHeader> backed by a leaked Box
    /// so tests don't need unsafe pointer arithmetic.
    fn dummy_object() -> NonNull<KernelObjectHeader>
    {
        let obj = Box::new(FrameObject {
            header: KernelObjectHeader::new(ObjectType::Frame),
            base: 0,
            size: 0x1000,
        });
        let raw = Box::into_raw(obj) as *mut KernelObjectHeader;
        // SAFETY: Box::into_raw never returns null.
        unsafe { NonNull::new_unchecked(raw) }
    }

    #[test]
    fn new_cspace_is_empty()
    {
        let cs = CSpace::new(0, 16384);
        assert_eq!(cs.populated_count(), 0);
        assert_eq!(cs.allocated_slots, 0);
    }

    #[test]
    fn slot_zero_is_null()
    {
        let mut cs = CSpace::new(0, 16384);
        // Force page 0 to be allocated by requesting slot 1.
        let idx = cs.allocate_slot().unwrap();
        assert_ne!(idx, 0, "allocate_slot must never return slot 0");
        // Slot 0 must exist and be null.
        let s = cs.slot(0).expect("slot 0 should exist after grow");
        assert!(s.is_null());
    }

    #[test]
    fn allocate_returns_nonzero_index()
    {
        let mut cs = CSpace::new(0, 16384);
        let idx = cs.allocate_slot().unwrap();
        assert_ne!(idx, 0);
    }

    #[test]
    fn allocate_and_lookup_round_trip()
    {
        let mut cs = CSpace::new(0, 16384);
        let obj = dummy_object();
        let idx = cs
            .insert_cap(CapTag::Frame, Rights::MAP | Rights::WRITE, obj)
            .unwrap();
        let slot = cs.slot(idx).unwrap();
        assert_eq!(slot.tag, CapTag::Frame);
        assert!(slot.rights.contains(Rights::MAP));
        assert!(slot.rights.contains(Rights::WRITE));
        assert_eq!(slot.object, Some(obj));
    }

    #[test]
    fn growth_across_l2_boundary()
    {
        // Allocate L2_SIZE - 1 slots (page 0 has 63 usable slots after skipping 0).
        let mut cs = CSpace::new(0, 16384);
        let mut indices = Vec::new();
        for _ in 0..(L2_SIZE - 1)
        {
            indices.push(cs.allocate_slot().unwrap());
        }
        // Next allocation must cross into page 1.
        let idx = cs.allocate_slot().unwrap();
        assert!(
            idx as usize >= L2_SIZE,
            "expected index in page 1 or beyond"
        );
        assert!(!indices.contains(&idx));
    }

    #[test]
    fn free_and_reallocate()
    {
        let mut cs = CSpace::new(0, 16384);
        let idx1 = cs.allocate_slot().unwrap();
        cs.free_slot(idx1);
        // After freeing, the next allocate should return the same index.
        let idx2 = cs.allocate_slot().unwrap();
        assert_eq!(idx1, idx2, "freed slot should be reused");
    }

    #[test]
    fn max_slots_enforced()
    {
        // max_slots = 63: exactly one page minus slot 0.
        let mut cs = CSpace::new(0, 63);
        for _ in 0..63
        {
            cs.allocate_slot().unwrap();
        }
        let err = cs.allocate_slot().unwrap_err();
        assert_eq!(err, CapError::OutOfSlots);
    }

    #[test]
    fn wx_violation_rejected()
    {
        let mut cs = CSpace::new(0, 16384);
        let obj = dummy_object();
        let err = cs
            .insert_cap(CapTag::Frame, Rights::WRITE | Rights::EXECUTE, obj)
            .unwrap_err();
        assert_eq!(err, CapError::WxViolation);
    }

    #[test]
    fn pre_allocate_succeeds()
    {
        let mut cs = CSpace::new(0, 16384);
        cs.pre_allocate(10).unwrap();
        assert!(cs.free_count >= 10);
    }

    #[test]
    fn populated_count_tracks_inserts()
    {
        let mut cs = CSpace::new(0, 16384);
        assert_eq!(cs.populated_count(), 0);
        let obj = dummy_object();
        cs.insert_cap(CapTag::Frame, Rights::MAP, obj).unwrap();
        assert_eq!(cs.populated_count(), 1);
    }
}
