// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// procmgr/src/main.rs

//! Seraph process manager — stub.
//!
//! procmgr is the userspace process lifecycle manager. It is the first service
//! started by init and handles all subsequent process creation, ELF loading,
//! and teardown. No process is created after early boot without going through
//! procmgr (except svcmgr, which holds raw fallback syscall capabilities to
//! restart procmgr if it crashes).
//!
//! This stub halts immediately. Full implementation is deferred.
//!
//! See `procmgr/README.md` for the design and IPC interface.

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
