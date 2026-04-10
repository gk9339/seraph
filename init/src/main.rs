// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// init/src/main.rs

//! Seraph init — bootstrap stub.
//!
//! Real init will be responsible for starting early system services (logd,
//! procmgr, devmgr, …) and then handing off to the service manager. That work
//! begins once the kernel reaches a sufficient level of completeness.
//!
//! Until then this binary is a minimal stub: it receives the kernel's initial
//! capability set and spins. It is the default binary loaded at boot
//! (`init=init` in boot.conf).
//!
//! For kernel testing, switch to `init=ktest` in boot.conf. See `ktest/README.md`.

#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[no_mangle]
pub extern "C" fn _start(_info_ptr: u64) -> !
{
    loop
    {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> !
{
    loop
    {
        core::hint::spin_loop();
    }
}
