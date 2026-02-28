// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/main.rs

//! Seraph microkernel — kernel entry point.
//!
//! Receives control from the bootloader after page tables are installed and
//! UEFI boot services have exited. See `docs/boot-protocol.md` for the CPU
//! state contract and `BootInfo` layout.
//!
//! This is a minimal verification stub. The full initialisation sequence
//! (11 phases) will be implemented as the kernel subsystems are built.
//! Verification: attach GDB, break at `kernel_entry`, confirm
//! `p/x *(uint32_t*)$rdi` (x86-64) or `p/x *(uint32_t*)$a0` (RISC-V) == 0x2.

#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
use core::panic::PanicInfo;

use boot_protocol::BootInfo;
use boot_protocol::BOOT_PROTOCOL_VERSION;

/// Kernel entry point.
///
/// Called by the bootloader with CPU state per `docs/boot-protocol.md`.
/// `boot_info` is the physical address of a populated [`BootInfo`] structure,
/// accessible before the kernel's own page tables are established because the
/// bootloader identity-maps the `BootInfo` region.
///
/// This stub reads `BootInfo.version` via a volatile read (preventing the
/// compiler from optimising it away) and halts. Set a GDB breakpoint here
/// to confirm the boot handoff succeeded and the protocol version is correct.
#[no_mangle]
pub extern "C" fn kernel_entry(boot_info: *const BootInfo) -> !
{
    // SAFETY: boot_info is provided by the bootloader and is guaranteed to be
    // a valid, non-null pointer to a populated BootInfo structure. The
    // bootloader identity-maps this region before jumping, so the physical
    // address is directly accessible. We perform a volatile read to prevent
    // the compiler from optimising away this access.
    let version = unsafe { core::ptr::read_volatile(&(*boot_info).version) };

    // Halt if the protocol version does not match. The kernel cannot safely
    // proceed with a mismatched bootloader.
    if version != BOOT_PROTOCOL_VERSION
    {
        halt_loop();
    }

    // GDB breakpoint target: if execution reaches here, boot handoff succeeded
    // and the protocol version matched. `b kernel_entry_verified` in GDB, then
    // `p version` to confirm the value is 2.
    halt_loop();
}

/// Disable interrupts and halt the CPU permanently.
///
/// Used as a terminal state for both the success path (waiting for GDB) and
/// the error path (mismatched protocol version). The `loop` prevents the
/// compiler from treating this as a function that returns.
fn halt_loop() -> !
{
    // SAFETY: The halt instruction puts the CPU into a low-power wait state.
    // Interrupts are disabled by the bootloader before kernel_entry is called,
    // so the halt is permanent. The loop is required to satisfy `-> !`.
    // The arch-specific instruction is in a temporary cfg guard in this stub
    // only; permanent arch code will use the arch trait in arch/mod.rs.
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
