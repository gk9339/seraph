// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// shared/runtime/src/lib.rs

//! Seraph userspace process runtime: `_start()` entry point and panic handler.
//!
//! Provides the `_start()` function linked into every normal userspace process
//! (everything except init and ktest, which have their own startup paths).
//!
//! `_start()` reads the [`ProcessInfo`] handover struct from the well-known
//! virtual address, constructs a [`StartupInfo`], and calls the binary's
//! `main()` function. If `main()` returns, `_start()` calls
//! `sys_thread_exit()` as a safety net.
//!
//! Binaries using this runtime define `main` as:
//! ```ignore
//! #[no_mangle]
//! fn main(startup: &process_abi::StartupInfo) -> ! { ... }
//! ```

#![no_std]

extern crate alloc;

pub mod heap;
pub mod log;

use process_abi::{
    process_info_ref, ProcessInfo, StartupInfo, PROCESS_ABI_VERSION, PROCESS_INFO_VADDR,
};

extern "Rust" {
    /// The binary's entry point. Defined by each userspace binary.
    fn main(startup: &StartupInfo) -> !;
}

/// Process entry point. Called by the kernel when the thread begins execution.
///
/// Reads [`ProcessInfo`] from [`PROCESS_INFO_VADDR`], validates the protocol
/// version, constructs [`StartupInfo`], and calls `main()`.
///
/// # Safety
///
/// The kernel (via procmgr) must have mapped a valid [`ProcessInfo`] page at
/// [`PROCESS_INFO_VADDR`] before starting this thread. The page must remain
/// mapped for the process's lifetime.
#[no_mangle]
pub extern "C" fn _start(_info_ptr: u64) -> !
{
    // SAFETY: procmgr maps a valid ProcessInfo page at PROCESS_INFO_VADDR
    // before starting the thread. The page is read-only and remains mapped
    // for the process's lifetime.
    let info: &ProcessInfo = unsafe { process_info_ref(PROCESS_INFO_VADDR) };

    if info.version != PROCESS_ABI_VERSION
    {
        // Version mismatch — cannot safely interpret the struct. Exit.
        syscall::thread_exit();
    }

    let startup = StartupInfo {
        ipc_buffer: info.ipc_buffer_vaddr as *mut u8,
        creator_endpoint: info.creator_endpoint_cap,
        self_thread: info.self_thread_cap,
        self_aspace: info.self_aspace_cap,
        self_cspace: info.self_cspace_cap,
    };

    // SAFETY: main is defined by the binary linking against this runtime.
    unsafe { main(&startup) }
}

/// Panic handler for userspace processes using this runtime.
///
/// Calls `sys_thread_exit()` — there is no recovery path for a panicking
/// userspace process.
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> !
{
    syscall::thread_exit();
}
