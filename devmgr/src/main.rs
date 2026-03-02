// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// devmgr/src/main.rs

//! Seraph device manager — stub.
//!
//! devmgr is responsible for platform enumeration, hardware discovery, and
//! binding drivers to devices. It is started early by init and manages the
//! lifetime of driver processes throughout the system.
//!
//! This stub halts immediately. Full implementation is deferred.
//!
//! See `devmgr/README.md` for the design and IPC interface.

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
