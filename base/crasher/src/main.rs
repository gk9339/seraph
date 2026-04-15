// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// base/crasher/src/main.rs

//! Deliberate-crash test service for svcmgr monitoring validation.
//!
//! Logs a startup message, sleeps for 10 seconds, then triggers a fault
//! (null pointer write). svcmgr should detect the death via `EventQueue`
//! notification and restart the service per its restart policy.

#![no_std]
#![no_main]

extern crate runtime;

use process_abi::StartupInfo;

#[no_mangle]
extern "Rust" fn main(_startup: &StartupInfo) -> !
{
    runtime::log!("crasher: alive");

    // Sleep for 10 seconds (no busy spin).
    // black_box prevents the optimizer from eliminating the sleep syscall.
    let _ = core::hint::black_box(syscall::thread_sleep(10_000));

    runtime::log!("crasher: triggering fault");

    // Trigger a fault: write to null pointer.
    // x86-64: #PF (vector 14) for unmapped page.
    // RISC-V: store page fault (scause 15).
    // SAFETY: deliberately invalid — this is the point.
    unsafe {
        core::ptr::write_volatile(core::ptr::null_mut::<u8>(), 0x42);
    }

    // Unreachable — the fault kills this thread.
    loop
    {
        let _ = syscall::thread_yield();
    }
}
