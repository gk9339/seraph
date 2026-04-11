// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// abi/process-abi/src/lib.rs

//! Userspace process startup ABI: the binary contract between a process creator
//! and the created process.
//!
//! Defines [`ProcessInfo`] (the `#[repr(C)]` handover struct placed at a
//! well-known virtual address before the new process runs), [`StartupInfo`]
//! (the Rust-native type passed to `main()`), and the shared [`CapDescriptor`]
//! and [`CapType`] types used by both this crate and `abi/init-protocol`.

#![no_std]

// ── Protocol version ─────────────────────────────────────────────────────────

/// Process ABI version. Incremented on any breaking change to the
/// [`ProcessInfo`] layout or field semantics.
pub const PROCESS_ABI_VERSION: u32 = 1;

// ── Address space constants ──────────────────────────────────────────────────

/// Virtual address where procmgr maps the read-only [`ProcessInfo`] page in
/// every new process's address space.
///
/// Analogous to `INIT_INFO_VADDR` in `abi/init-protocol`. Placed in the upper
/// half of the user address range, below the stack.
pub const PROCESS_INFO_VADDR: u64 = 0x0000_7FFF_FFFF_0000;

/// Virtual address of the top of a normal process's user stack.
///
/// `PROCESS_STACK_PAGES` pages are mapped immediately below this address.
/// One additional guard page (unmapped) sits below the stack.
pub const PROCESS_STACK_TOP: u64 = 0x0000_7FFF_FFFF_E000;

/// Number of 4 KiB pages in a normal process's user stack (16 KiB total).
pub const PROCESS_STACK_PAGES: usize = 4;

// ── CapDescriptor ────────────────────────────────────────────────────────────

/// Describes a single capability in a process's `CSpace`.
///
/// Used in the variable-length descriptor array following both [`ProcessInfo`]
/// and `InitInfo` (from `abi/init-protocol`). Each entry identifies the slot
/// index, capability type, and type-specific metadata so the process can
/// identify what each capability slot represents without probing.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CapDescriptor
{
    /// `CSpace` slot index.
    pub slot: u32,

    /// Capability type discriminant. See [`CapType`].
    pub cap_type: CapType,

    /// Padding for alignment; must be zero.
    #[doc(hidden)]
    pub pad: [u8; 3],

    /// Type-specific primary metadata:
    /// - `Frame`: physical base address
    /// - `MmioRegion`: physical base address
    /// - `Interrupt`: IRQ line number
    /// - `IoPortRange`: I/O port base
    /// - `SchedControl`: 0 (unused)
    pub aux0: u64,

    /// Type-specific secondary metadata:
    /// - `Frame`: size in bytes
    /// - `MmioRegion`: size in bytes
    /// - `Interrupt`: flags
    /// - `IoPortRange`: port count
    /// - `SchedControl`: 0 (unused)
    pub aux1: u64,
}

// ── CapType ──────────────────────────────────────────────────────────────────

/// Capability type discriminant for [`CapDescriptor`].
///
/// Discriminant values match the kernel's `CapTag` enum for the types that
/// appear in initial `CSpace` populations. Types that are never present at
/// boot (Endpoint, Signal, Thread, etc.) are omitted.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapType
{
    /// Physical memory frame(s). Matches `CapTag::Frame = 1`.
    Frame = 1,
    /// Hardware interrupt line. Matches `CapTag::Interrupt = 6`.
    Interrupt = 6,
    /// Memory-mapped I/O region. Matches `CapTag::MmioRegion = 7`.
    MmioRegion = 7,
    /// x86-64 I/O port range. Matches `CapTag::IoPortRange = 11`.
    IoPortRange = 11,
    /// Scheduling control authority. Matches `CapTag::SchedControl = 12`.
    SchedControl = 12,
    /// SBI forwarding authority (RISC-V only). Matches `CapTag::SbiControl = 13`.
    SbiControl = 13,
}

// ── ProcessInfo ──────────────────────────────────────────────────────────────

/// Procmgr-to-process handover structure.
///
/// Placed at [`PROCESS_INFO_VADDR`] (one 4 KiB page, read-only) before the
/// new process begins execution. The fixed-size header is followed by a
/// variable-length [`CapDescriptor`] array; the array starts at byte offset
/// [`ProcessInfo::cap_descriptors_offset`] from the start of this struct.
///
/// All slot indices refer to the process's own `CSpace`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProcessInfo
{
    /// Protocol version. Must equal [`PROCESS_ABI_VERSION`].
    pub version: u32,

    // ── Process identity ─────────────────────────────────────────────
    /// `CSpace` slot of the process's own Thread capability (Control right).
    pub self_thread_cap: u32,

    /// `CSpace` slot of the process's own `AddressSpace` capability.
    pub self_aspace_cap: u32,

    /// `CSpace` slot of the process's own `CSpace` capability.
    pub self_cspace_cap: u32,

    // ── IPC ──────────────────────────────────────────────────────────
    /// Virtual address of the pre-mapped IPC buffer page.
    ///
    /// Every thread requires a registered IPC buffer for extended message
    /// payloads. procmgr maps this page and records it here; the process
    /// calls `SYS_IPC_BUFFER_SET` with this address on startup.
    pub ipc_buffer_vaddr: u64,

    /// `CSpace` slot of an IPC endpoint back to the creating service.
    ///
    /// For processes created by procmgr directly, this is an endpoint to
    /// procmgr. Zero if no parent endpoint is provided.
    pub parent_endpoint_cap: u32,

    // ── Initial capabilities ─────────────────────────────────────────
    /// First `CSpace` slot containing service-specific initial capabilities.
    pub initial_caps_base: u32,

    /// Number of initial capability slots.
    pub initial_caps_count: u32,

    /// Number of [`CapDescriptor`] entries following this struct.
    pub cap_descriptor_count: u32,

    /// Byte offset from the start of this struct to the first
    /// [`CapDescriptor`] entry.
    pub cap_descriptors_offset: u32,

    // ── Startup message ──────────────────────────────────────────────
    /// Byte offset from the start of this struct to the startup message.
    /// Zero if no startup message is present.
    pub startup_message_offset: u32,

    /// Length of the startup message in bytes. Zero if absent.
    pub startup_message_len: u32,

    /// Padding to maintain 8-byte alignment for the trailing
    /// [`CapDescriptor`] array.
    // pub_underscore_fields: field is part of the `#[repr(C)]` ABI layout;
    // must be public so producers (procmgr) can set it, but has no semantic
    // meaning for consumers.
    #[allow(clippy::pub_underscore_fields)]
    pub _pad: u32,
}

// ── StartupInfo ──────────────────────────────────────────────────────────────

/// Rust-native startup information passed to `main()`.
///
/// Constructed by `_start()` from either [`ProcessInfo`] (normal processes)
/// or `InitInfo` (init/ktest). References borrow from the handover page,
/// which remains mapped read-only for the process's lifetime.
pub struct StartupInfo<'a>
{
    /// Capability descriptors for initial capabilities.
    pub initial_caps: &'a [CapDescriptor],

    /// Virtual address of the IPC buffer page.
    pub ipc_buffer: *mut u8,

    /// `CSpace` slot of the parent endpoint. Zero if none.
    pub parent_endpoint: u32,

    /// Startup message bytes. Empty slice if none.
    pub startup_message: &'a [u8],

    /// `CSpace` slot of own Thread capability.
    pub self_thread: u32,

    /// `CSpace` slot of own `AddressSpace` capability.
    pub self_aspace: u32,

    /// `CSpace` slot of own `CSpace` capability.
    pub self_cspace: u32,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Return the [`CapDescriptor`] slice from a [`ProcessInfo`] page.
///
/// # Safety
///
/// `info` must point into the read-only [`ProcessInfo`] page mapped at
/// [`PROCESS_INFO_VADDR`]. The page must contain at least
/// `info.cap_descriptors_offset + info.cap_descriptor_count * size_of::<CapDescriptor>()`
/// valid bytes.
#[must_use]
pub unsafe fn cap_descriptors(info: &ProcessInfo) -> &[CapDescriptor]
{
    if info.cap_descriptor_count == 0
    {
        return &[];
    }
    let base = core::ptr::from_ref::<ProcessInfo>(info).cast::<u8>();
    // SAFETY: caller guarantees the ProcessInfo page contains valid
    // CapDescriptor data at the specified offset and count.
    // cast_ptr_alignment: the ProcessInfo page is page-aligned (4096-byte),
    // and cap_descriptors_offset is set by procmgr to maintain CapDescriptor
    // alignment (8-byte).
    #[allow(clippy::cast_ptr_alignment)]
    unsafe {
        let ptr = base
            .add(info.cap_descriptors_offset as usize)
            .cast::<CapDescriptor>();
        core::slice::from_raw_parts(ptr, info.cap_descriptor_count as usize)
    }
}

/// Return the startup message bytes from a [`ProcessInfo`] page.
///
/// # Safety
///
/// `info` must point into the read-only [`ProcessInfo`] page mapped at
/// [`PROCESS_INFO_VADDR`]. The page must contain at least
/// `info.startup_message_offset + info.startup_message_len` valid bytes.
#[must_use]
pub unsafe fn startup_message(info: &ProcessInfo) -> &[u8]
{
    if info.startup_message_len == 0 || info.startup_message_offset == 0
    {
        return &[];
    }
    let base = core::ptr::from_ref::<ProcessInfo>(info).cast::<u8>();
    // SAFETY: caller guarantees the ProcessInfo page contains valid startup
    // message data at the specified offset and length.
    unsafe {
        core::slice::from_raw_parts(
            base.add(info.startup_message_offset as usize),
            info.startup_message_len as usize,
        )
    }
}
