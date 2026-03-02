// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// fs/fat/src/main.rs

//! Seraph FAT filesystem driver — stub.
//!
//! fatfs implements FAT12/16/32 filesystem support for vfsd. It registers as
//! a filesystem driver and handles mount/unmount and file I/O requests from
//! vfsd over IPC.
//!
//! This stub halts immediately. Full implementation is deferred.

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
