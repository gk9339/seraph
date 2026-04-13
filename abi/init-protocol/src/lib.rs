// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// abi/init-protocol/src/lib.rs

//! Init entry protocol — kernel-to-init handover contract.
//!
//! This crate defines the binary interface between the kernel (Phase 7/9) and
//! the init process. The kernel populates an [`InitInfo`] structure in a
//! read-only page mapped into init's address space at [`INIT_INFO_VADDR`] and
//! passes that address as init's sole entry argument (`rdi` on x86-64, `a0`
//! on RISC-V).
//!
//! # Versioning
//!
//! [`INIT_PROTOCOL_VERSION`] is incremented on any breaking change to the
//! `InitInfo` layout, field semantics, or `CSpace` population order. Init MUST
//! check `info.version == INIT_PROTOCOL_VERSION` before accessing any fields.
//!
//! # Rules
//! - `no_std`; builds in `no_std`.
//! - No inline assembly.
//! - All cross-boundary types are `#[repr(C)]` with stable layout.
//! - No dependencies outside `core`.

#![no_std]

// ── Protocol version ─────────────────────────────────────────────────────────

/// Init protocol version. Incremented on any breaking layout or semantic change.
///
/// v3: Added `cmdline_offset`, `cmdline_len`, and `sbi_control_cap` for kernel
///     command line passthrough and RISC-V SBI forwarding.
pub const INIT_PROTOCOL_VERSION: u32 = 4;

// ── Address space constants ──────────────────────────────────────────────────

/// Virtual address where the kernel maps the read-only [`InitInfo`] page.
///
/// Placed below the stack and its guard page. The layout is:
///
/// ```text
/// INIT_INFO_VADDR          → InitInfo (4 KiB, read-only)
/// INIT_INFO_VADDR + 0x1000 → guard page (unmapped)
/// INIT_STACK_TOP - N*4KiB  → stack pages (read-write, N = INIT_STACK_PAGES)
/// INIT_STACK_TOP           → top of stack
/// ```
pub const INIT_INFO_VADDR: u64 = 0x7FFF_FFFF_8000;

/// Virtual address of the top of init's user stack.
///
/// `INIT_STACK_PAGES` pages are mapped immediately below this address.
/// One additional guard page (unmapped) sits below the stack.
pub const INIT_STACK_TOP: u64 = 0x7FFF_FFFF_E000;

/// Number of 4 KiB pages in init's user stack (16 KiB total).
pub const INIT_STACK_PAGES: usize = 4;

// ── InitInfo ─────────────────────────────────────────────────────────────────

/// Kernel-to-init handover structure.
///
/// Placed at [`INIT_INFO_VADDR`] (one 4 KiB page, read-only). The fixed-size
/// header is followed by a variable-length [`CapDescriptor`] array; the array
/// starts at byte offset [`InitInfo::cap_descriptors_offset`] from the start
/// of this struct.
///
/// All slot indices refer to init's root `CSpace` (`CSpace` ID 0).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InitInfo
{
    /// Protocol version. Must equal [`INIT_PROTOCOL_VERSION`].
    pub version: u32,

    /// Number of [`CapDescriptor`] entries in the descriptor array.
    pub cap_descriptor_count: u32,

    // ── Init's own resources ─────────────────────────────────────────────
    /// Slot index of init's own `AddressSpace` capability.
    pub aspace_cap: u32,

    /// Slot index of the `SchedControl` capability.
    pub sched_control_cap: u32,

    // ── CSpace slot ranges (contiguous) ──────────────────────────────────
    /// First slot index of usable physical memory `Frame` capabilities.
    pub memory_frame_base: u32,
    /// Number of usable memory `Frame` capabilities.
    pub memory_frame_count: u32,

    /// First slot index of init's ELF segment `Frame` capabilities.
    pub segment_frame_base: u32,
    /// Number of segment `Frame` capabilities.
    pub segment_frame_count: u32,

    /// First slot index of boot module `Frame` capabilities.
    ///
    /// Boot modules are ELF images for early services (procmgr, devmgr, etc.)
    /// loaded by the bootloader. Currently not populated (count = 0) until the
    /// boot protocol is extended with module metadata.
    pub module_frame_base: u32,
    /// Number of boot module `Frame` capabilities.
    pub module_frame_count: u32,

    /// First slot index of hardware resource capabilities (MMIO, IRQ, I/O port).
    pub hw_cap_base: u32,
    /// Number of hardware resource capabilities.
    pub hw_cap_count: u32,

    /// Byte offset from the start of this struct to the first [`CapDescriptor`].
    ///
    /// The descriptor array contains `cap_descriptor_count` entries, one per
    /// capability in the hardware resource and memory frame ranges. Init uses
    /// these to identify what each capability slot represents without probing.
    pub cap_descriptors_offset: u32,

    /// Slot index of init's own `Thread` capability (CONTROL right).
    ///
    /// Allows init to bind I/O port ranges to itself (`ioport_bind`), set its
    /// own priority and affinity, and delegate thread authority to child services.
    pub thread_cap: u32,

    // ── Command line (added in protocol version 3) ──────────────────────
    /// Byte offset from the start of this struct to the kernel command line.
    ///
    /// The command line is placed after the [`CapDescriptor`] array within the
    /// same 4 KiB page. Zero if no command line is present.
    pub cmdline_offset: u32,

    /// Length of the command line in bytes (no null terminator). Zero if absent.
    pub cmdline_len: u32,

    // ── RISC-V SBI forwarding (added in protocol version 3) ─────────────
    /// Slot index of the `SbiControl` capability (RISC-V only).
    ///
    /// Grants authority to forward SBI calls from userspace through the kernel.
    /// Zero on x86-64 (no SBI concept).
    pub sbi_control_cap: u32,

    /// Slot index of init's own `CSpace` capability.
    ///
    /// Init needs this to create threads bound to its own `CSpace` (e.g. a log
    /// thread that shares init's capability namespace). Added in protocol v4.
    pub cspace_cap: u32,
}

// ── CapDescriptor / CapType (re-exported from process-abi) ───────────────────

// Canonical definitions live in `abi/process-abi`. Re-exported here so
// existing consumers (kernel, ktest) continue to compile unmodified.
pub use process_abi::{CapDescriptor, CapType};

// ── Helpers ─────────────────────────────────────────────────────────────

/// Return the kernel command line as a byte slice from the [`InitInfo`] page.
///
/// # Safety
/// `info` must point into the read-only [`InitInfo`] page mapped by the kernel
/// at [`INIT_INFO_VADDR`]. The page must contain at least
/// `info.cmdline_offset + info.cmdline_len` valid bytes.
#[must_use]
pub unsafe fn cmdline_bytes(info: &InitInfo) -> &[u8]
{
    if info.cmdline_len == 0 || info.cmdline_offset == 0
    {
        return &[];
    }
    let base = core::ptr::from_ref::<InitInfo>(info).cast::<u8>();
    // SAFETY: caller guarantees the InitInfo page contains valid cmdline data
    // at the specified offset and length, populated by the kernel in Phase 9.
    unsafe {
        core::slice::from_raw_parts(
            base.add(info.cmdline_offset as usize),
            info.cmdline_len as usize,
        )
    }
}
