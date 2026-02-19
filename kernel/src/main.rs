//! Seraph microkernel — kernel entry point.
//!
//! See `docs/boot-protocol.md` for the CPU state and `BootInfo` contract
//! guaranteed by the bootloader before `kernel_entry` is called.
//! See `kernel/docs/initialization.md` for the 11-phase boot sequence.

#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
use core::panic::PanicInfo;

/// Kernel entry point.
///
/// Called by the bootloader after establishing the CPU state described in
/// `docs/boot-protocol.md`. Receives a pointer to the `BootInfo` structure
/// whose physical address is in `rdi` (x86-64) or `a0` (RISC-V).
///
/// This function must not return.
#[no_mangle]
pub extern "C" fn kernel_entry(_boot_info: *const core::ffi::c_void) -> !
{
    loop {}
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> !
{
    loop {}
}
