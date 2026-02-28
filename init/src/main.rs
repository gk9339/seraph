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
/// This stub halts immediately. Set a GDB breakpoint here to confirm
/// kernel-to-userspace handoff succeeded.
#[no_mangle]
pub extern "C" fn _start() -> !
{
    halt_loop();
}

/// Disable interrupts and halt the CPU permanently.
fn halt_loop() -> !
{
    loop
    {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack));
        }

        #[cfg(target_arch = "riscv64")]
        unsafe {
            core::arch::asm!("wfi", options(nomem, nostack));
        }
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> !
{
    halt_loop();
}
