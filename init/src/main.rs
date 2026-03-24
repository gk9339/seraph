// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// init/src/main.rs

//! Seraph init — minimal stub.
//!
//! PID 1 service manager stub. Receives control from the kernel after
//! Phase 9 of its initialisation sequence. This stub halts immediately;
//! the full init implementation is deferred.

#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
use core::panic::PanicInfo;

/// Init entry point. Called by the kernel as the first userspace process.
///
/// This stub spins forever. Set a GDB breakpoint here to confirm
/// kernel-to-userspace handoff succeeded.
#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn _start() -> !
{
    halt_loop();
}

/// Spin forever. Placeholder until init is fully implemented.
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
    halt_loop();
}
