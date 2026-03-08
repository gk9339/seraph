// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/cap/object.rs

//! Kernel object types backing Phase 7 capabilities.
//!
//! Each struct has a [`KernelObjectHeader`] as its first field at offset 0
//! (`#[repr(C)]`), so a `*mut ConcreteObject` can safely be cast to
//! `*mut KernelObjectHeader` and back.
//!
//! ## Allocation pattern
//!
//! ```text
//! Box::new(FrameObject { ... })
//!   → Box::into_raw()            // *mut FrameObject
//!   → cast to *mut KernelObjectHeader  // safe: header at offset 0
//!   → NonNull::new_unchecked()
//! ```
//!
//! Deallocation (future phases): read `header.obj_type` from the raw pointer,
//! then reconstruct the original `Box<ConcreteObject>` and drop it.
//!
//! ## Sizes (verified by tests below)
//!
//! | Type                | Size |
//! |---------------------|------|
//! | KernelObjectHeader  |  8 B |
//! | FrameObject         | 24 B |
//! | MmioRegionObject    | 32 B |
//! | InterruptObject     | 16 B |
//! | IoPortRangeObject   | 12 B |
//! | SchedControlObject  |  8 B |

use core::sync::atomic::{AtomicU32, Ordering};

// ── ObjectType ────────────────────────────────────────────────────────────────

/// Discriminant for the concrete type behind a `*mut KernelObjectHeader`.
///
/// Used during deallocation to reconstruct the original `Box<ConcreteObject>`.
/// Values must not be renumbered after assignment.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectType
{
    Frame = 0,
    MmioRegion = 1,
    Interrupt = 2,
    IoPortRange = 3,
    SchedControl = 4,
}

// ── KernelObjectHeader ────────────────────────────────────────────────────────

/// Common header at offset 0 of every kernel object.
///
/// The `ref_count` tracks how many capability slots reference this object.
/// When `dec_ref` returns 0, the object has no remaining references and can
/// be freed (future phases will handle deallocation via `obj_type`).
///
/// `#[repr(C)]` with size 8 B, alignment 4. All concrete object structs place
/// this as their first field so pointer casts are safe.
#[repr(C)]
pub struct KernelObjectHeader
{
    /// Reference count; starts at 1 when created.
    pub ref_count: AtomicU32,
    /// Concrete type, for use during deallocation.
    pub obj_type: ObjectType,
    pub _pad: [u8; 3],
}

impl KernelObjectHeader
{
    /// Construct a new header with `ref_count = 1`.
    pub fn new(obj_type: ObjectType) -> Self
    {
        Self {
            ref_count: AtomicU32::new(1),
            obj_type,
            _pad: [0; 3],
        }
    }

    /// Increment the reference count. Call when a new capability slot is
    /// derived pointing to this object.
    pub fn inc_ref(&self)
    {
        self.ref_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement the reference count and return the new value.
    ///
    /// Returns 0 when the object has no remaining capability references; the
    /// caller is responsible for freeing the object at that point.
    pub fn dec_ref(&self) -> u32
    {
        self.ref_count.fetch_sub(1, Ordering::Release) - 1
    }
}

// ── Concrete object types ─────────────────────────────────────────────────────

/// Kernel object for a contiguous physical memory range (Frame capability).
#[repr(C)]
pub struct FrameObject
{
    pub header: KernelObjectHeader,
    /// Physical base address of the region.
    pub base: u64,
    /// Size of the region in bytes.
    pub size: u64,
}

/// Kernel object for a memory-mapped I/O region (MmioRegion capability).
#[repr(C)]
pub struct MmioRegionObject
{
    pub header: KernelObjectHeader,
    /// Physical base address of the MMIO region.
    pub base: u64,
    /// Size of the MMIO region in bytes.
    pub size: u64,
    /// Flags from the platform resource entry (bit 0: write-combine).
    pub flags: u32,
    pub _pad: u32,
}

/// Kernel object for a hardware interrupt line (Interrupt capability).
#[repr(C)]
pub struct InterruptObject
{
    pub header: KernelObjectHeader,
    /// Interrupt number (GSI on x86-64, PLIC source on RISC-V).
    pub irq_id: u32,
    /// Flags from the platform resource entry (edge/level, polarity).
    pub flags: u32,
}

/// Kernel object for an x86-64 I/O port range (IoPortRange capability).
#[repr(C)]
pub struct IoPortRangeObject
{
    pub header: KernelObjectHeader,
    /// First port number in the range.
    pub base: u16,
    /// Number of consecutive ports.
    pub size: u16,
    pub _pad: u32,
}

/// Kernel object for scheduling control authority (SchedControl capability).
///
/// There is exactly one SchedControl object, created at boot.
#[repr(C)]
pub struct SchedControlObject
{
    pub header: KernelObjectHeader,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests
{
    use super::*;
    use core::mem::{offset_of, size_of};

    // Verify header is at offset 0 in each concrete type — required for safe
    // pointer casts from *mut ConcreteObject to *mut KernelObjectHeader.
    #[test]
    fn frame_object_header_at_offset_zero()
    {
        assert_eq!(offset_of!(FrameObject, header), 0);
    }

    #[test]
    fn mmio_object_header_at_offset_zero()
    {
        assert_eq!(offset_of!(MmioRegionObject, header), 0);
    }

    #[test]
    fn interrupt_object_header_at_offset_zero()
    {
        assert_eq!(offset_of!(InterruptObject, header), 0);
    }

    #[test]
    fn ioport_object_header_at_offset_zero()
    {
        assert_eq!(offset_of!(IoPortRangeObject, header), 0);
    }

    #[test]
    fn sched_control_object_header_at_offset_zero()
    {
        assert_eq!(offset_of!(SchedControlObject, header), 0);
    }

    #[test]
    fn struct_sizes()
    {
        assert_eq!(size_of::<KernelObjectHeader>(), 8);
        assert_eq!(size_of::<FrameObject>(), 24);
        assert_eq!(size_of::<MmioRegionObject>(), 32);
        assert_eq!(size_of::<InterruptObject>(), 16);
        assert_eq!(size_of::<IoPortRangeObject>(), 16); // 8 header + 4 ports + 4 pad
        assert_eq!(size_of::<SchedControlObject>(), 8);
    }

    #[test]
    fn header_ref_count_lifecycle()
    {
        let h = KernelObjectHeader::new(ObjectType::Frame);
        assert_eq!(h.ref_count.load(core::sync::atomic::Ordering::Relaxed), 1);
        h.inc_ref();
        assert_eq!(h.ref_count.load(core::sync::atomic::Ordering::Relaxed), 2);
        let after_dec = h.dec_ref();
        assert_eq!(after_dec, 1);
        let after_dec2 = h.dec_ref();
        assert_eq!(after_dec2, 0);
    }
}
