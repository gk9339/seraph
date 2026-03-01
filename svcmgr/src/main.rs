// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// svcmgr/src/main.rs

//! Seraph service manager — stub.
//!
//! svcmgr monitors running services, restarts them on crash, and holds raw
//! process-creation syscall capabilities as a fallback to reconstruct procmgr
//! if procmgr itself crashes. svcmgr is started by init before init exits, and
//! runs for the lifetime of the system.
//!
//! This stub halts immediately. Full implementation is deferred.
//!
//! See `svcmgr/README.md` for the design and restart policy.

#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
use core::panic::PanicInfo;

#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn _start() -> !
{
    halt_loop();
}

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
