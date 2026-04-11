// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// vfsd/src/main.rs

//! Seraph virtual filesystem daemon — stub.
//!
//! vfsd presents a unified virtual filesystem namespace to all other processes.
//! It manages filesystem driver instances (fatfs, ext4, tmpfs, …) and routes
//! VFS IPC requests to the appropriate backing driver.
//!
//! This stub idles immediately. Full implementation is deferred to Tier 4.
//!
//! See `vfsd/README.md` for the design and IPC interface.

#![no_std]
#![no_main]

// Link shared/runtime to get _start() and panic_handler.
extern crate runtime;

use process_abi::StartupInfo;

/// VFS daemon entry point.
///
/// TODO: real vfsd implementation (Tier 4) — mount table, path resolution,
/// fs driver lifecycle, namespace IPC endpoint.
#[no_mangle]
extern "Rust" fn main(startup: &StartupInfo) -> !
{
    // Register IPC buffer so future IPC operations work.
    let _ = syscall::ipc_buffer_set(startup.ipc_buffer as u64);

    // Idle until real implementation arrives.
    loop
    {
        let _ = syscall::thread_yield();
    }
}
