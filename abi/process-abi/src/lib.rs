// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// abi/process-abi/src/lib.rs

//! Userspace process startup ABI: the binary contract between a process creator
//! and the created process.
//!
//! Defines [`ProcessInfo`] (the `#[repr(C)]` handover struct placed at a
//! well-known virtual address before the new process runs) and [`StartupInfo`]
//! (the Rust-native type passed to `main()`).
//!
//! The ABI delivers only the kernel-object caps (thread/aspace/cspace), the
//! pre-mapped IPC buffer, and the creator endpoint cap. All service-specific
//! capabilities (log, registry, device caps, etc.) are requested by the child
//! at startup over IPC on the creator endpoint — see
//! `shared/runtime/src/bootstrap.rs` and `shared/ipc/src/lib.rs::bootstrap`.

#![no_std]

// ── Protocol version ─────────────────────────────────────────────────────────

/// Process ABI version. Incremented on any breaking change to the
/// [`ProcessInfo`] layout or field semantics.
pub const PROCESS_ABI_VERSION: u32 = 2;

// ── Address space constants ──────────────────────────────────────────────────

/// Virtual address where procmgr maps the read-only [`ProcessInfo`] page in
/// every new process's address space.
pub const PROCESS_INFO_VADDR: u64 = 0x0000_7FFF_FFFF_0000;

/// Virtual address of the top of a normal process's user stack.
///
/// `PROCESS_STACK_PAGES` pages are mapped immediately below this address.
/// One additional guard page (unmapped) sits below the stack.
pub const PROCESS_STACK_TOP: u64 = 0x0000_7FFF_FFFF_E000;

/// Number of 4 KiB pages in a normal process's user stack (16 KiB total).
pub const PROCESS_STACK_PAGES: usize = 4;

// ── ProcessInfo ──────────────────────────────────────────────────────────────

/// Creator-to-process handover structure.
///
/// Placed at [`PROCESS_INFO_VADDR`] (one 4 KiB page, read-only) before the
/// new process begins execution.
///
/// All slot indices refer to the process's own `CSpace`. Beyond the kernel-
/// object self-caps and the creator endpoint, no service-specific capabilities
/// are delivered through this page — the child requests them from its creator
/// over IPC at startup (see `ipc::bootstrap`).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProcessInfo
{
    /// Protocol version. Must equal [`PROCESS_ABI_VERSION`].
    pub version: u32,

    /// `CSpace` slot of the process's own Thread capability (Control right).
    pub self_thread_cap: u32,

    /// `CSpace` slot of the process's own `AddressSpace` capability.
    pub self_aspace_cap: u32,

    /// `CSpace` slot of the process's own `CSpace` capability.
    pub self_cspace_cap: u32,

    /// Virtual address of the pre-mapped IPC buffer page.
    ///
    /// Every thread requires a registered IPC buffer for extended message
    /// payloads. The creator maps this page and records it here; the process
    /// calls `SYS_IPC_BUFFER_SET` with this address on startup.
    pub ipc_buffer_vaddr: u64,

    /// `CSpace` slot of a tokened IPC endpoint back to the creating service's
    /// bootstrap handler.
    ///
    /// The child calls `ipc::bootstrap::REQUEST` on this endpoint in a loop to
    /// receive its service-specific capability set. Zero if no creator
    /// endpoint is provided (child operates without bootstrap caps).
    pub creator_endpoint_cap: u32,
}

// ── StartupInfo ──────────────────────────────────────────────────────────────

/// Rust-native startup information passed to `main()`.
///
/// Constructed by `_start()` from [`ProcessInfo`].
pub struct StartupInfo
{
    /// Virtual address of the IPC buffer page.
    pub ipc_buffer: *mut u8,

    /// `CSpace` slot of the creator endpoint. Zero if none.
    pub creator_endpoint: u32,

    /// `CSpace` slot of own Thread capability.
    pub self_thread: u32,

    /// `CSpace` slot of own `AddressSpace` capability.
    pub self_aspace: u32,

    /// `CSpace` slot of own `CSpace` capability.
    pub self_cspace: u32,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Cast a page-aligned virtual address to a `ProcessInfo` reference.
///
/// Encapsulates the `u64 → *const ProcessInfo` cast with alignment validation,
/// eliminating per-site `#[allow(clippy::cast_ptr_alignment)]` annotations.
///
/// # Safety
///
/// `va` must point to a valid, mapped [`ProcessInfo`] page. The page must
/// remain mapped for the lifetime of the returned reference.
#[must_use]
pub unsafe fn process_info_ref(va: u64) -> &'static ProcessInfo
{
    debug_assert!(va.is_multiple_of(4096), "ProcessInfo VA not page-aligned");
    // SAFETY: caller guarantees va points to a valid, mapped ProcessInfo page.
    // cast_ptr_alignment: va is page-aligned (4096-byte), exceeding
    // ProcessInfo's alignment requirement.
    #[allow(clippy::cast_ptr_alignment)]
    unsafe {
        &*(va as *const ProcessInfo)
    }
}

/// Cast a page-aligned virtual address to a mutable `ProcessInfo` reference.
///
/// # Safety
///
/// `va` must point to a writable, page-aligned mapping of a [`ProcessInfo`]
/// page. The page must remain mapped for the lifetime of the returned
/// reference.
#[must_use]
pub unsafe fn process_info_mut(va: u64) -> &'static mut ProcessInfo
{
    debug_assert!(va.is_multiple_of(4096), "ProcessInfo VA not page-aligned");
    // SAFETY: caller guarantees va points to a writable, mapped ProcessInfo
    // page. cast_ptr_alignment: va is page-aligned (4096-byte).
    #[allow(clippy::cast_ptr_alignment)]
    unsafe {
        &mut *(va as *mut ProcessInfo)
    }
}
