// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// init/src/main.rs

//! Seraph init — Phase 10 test program.
//!
//! Exercises the syscall → capability lookup → IPC → return path by running
//! a signal round-trip:
//! 1. Register an IPC buffer page.
//! 2. Create a Signal capability.
//! 3. Send bits 0x42 to self.
//! 4. Wait on the signal — should return 0x42 immediately (bits already set).
//! 5. Print success message and exit.

#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
use core::panic::PanicInfo;

use seraph_syscall::{
    cap_create_signal, debug_log, ipc_buffer_set, signal_send, signal_wait, thread_exit,
};

/// Static IPC buffer — 4 KiB, page-aligned.
///
/// `#[repr(align(4096))]` guarantees the buffer is at a page boundary so
/// the kernel's page-alignment check in `SYS_IPC_BUFFER_SET` passes.
#[repr(C, align(4096))]
struct IpcBuf([u64; 512]);

static IPC_BUF: IpcBuf = IpcBuf([0u64; 512]);

/// Init entry point. Called by the kernel after Phase 9 init.
#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn _start() -> !
{
    run();
}

fn run() -> !
{
    debug_log("init: starting").ok();

    // Register the IPC buffer page so the kernel knows where our data words live.
    ipc_buffer_set(&IPC_BUF as *const IpcBuf as u64)
        .unwrap_or_else(|_| debug_log("init: ipc_buffer_set failed").ok().map(|_| {}).unwrap_or(()));

    // Create a Signal capability (stored in our CSpace at the returned index).
    let sig = cap_create_signal().unwrap_or_else(|_e| {
        debug_log("init: cap_create_signal failed").ok();
        halt_loop()
    });

    // Send bits to self. Since we have no waiter yet, the bits are stored in
    // the signal's bitmask.
    signal_send(sig, 0x42).unwrap_or_else(|_e| {
        debug_log("init: signal_send failed").ok();
        halt_loop()
    });

    // Wait on the signal — bits are already set, so this returns immediately
    // without blocking.
    let bits = signal_wait(sig).unwrap_or_else(|_e| {
        debug_log("init: signal_wait failed").ok();
        halt_loop()
    });

    if bits == 0x42
    {
        debug_log("init: signal round-trip passed").ok();
    }
    else
    {
        debug_log("init: signal round-trip FAILED (wrong bits)").ok();
    }

    thread_exit()
}

/// Spin forever. Used on fatal error paths.
fn halt_loop() -> !
{
    loop
    {
        core::hint::spin_loop();
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> !
{
    debug_log("init: panic").ok();
    halt_loop()
}
