// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// base/crasher/src/main.rs

//! Deliberate-crash test service for svcmgr monitoring validation.
//!
//! Bootstraps a log endpoint from its creator (init on first start, svcmgr on
//! restarts), logs a startup message, sleeps for 2 seconds, then triggers a
//! fault (null pointer write). svcmgr should detect the death via
//! `EventQueue` notification and restart the service per its restart policy.

#![no_std]
#![no_main]

extern crate runtime;

use process_abi::StartupInfo;

#[no_mangle]
extern "Rust" fn main(startup: &StartupInfo) -> !
{
    if syscall::ipc_buffer_set(startup.ipc_buffer as u64).is_err()
    {
        syscall::thread_exit();
    }

    // Bootstrap: one round, one cap (log_ep).
    if startup.creator_endpoint != 0
    {
        // SAFETY: IPC buffer is registered and page-aligned.
        let ipc = unsafe { ipc::IpcBuf::from_bytes(startup.ipc_buffer) };
        if let Ok(round) = ipc::bootstrap::request_round(startup.creator_endpoint, ipc)
        {
            if round.cap_count >= 1
            {
                runtime::log::log_init(round.caps[0], startup.ipc_buffer);
            }
        }
    }

    runtime::log!("crasher: alive");

    let _ = syscall::thread_sleep(2_000);

    runtime::log!("crasher: triggering fault");

    // Trigger a fault: write to null pointer.
    // x86-64: #PF (vector 14) for unmapped page.
    // RISC-V: store page fault (scause 15).
    // SAFETY: deliberately invalid — this is the point.
    unsafe {
        core::ptr::write_volatile(core::ptr::null_mut::<u8>(), 0x42);
    }

    // SAFETY: unreachable — the write above faults and the kernel kills this thread.
    unsafe { core::hint::unreachable_unchecked() }
}
