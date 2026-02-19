// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// boot/loader/src/main.rs

//! Seraph UEFI bootloader.
//!
//! Loads the kernel ELF and boot modules, establishes initial page tables,
//! parses platform firmware tables into structured `PlatformResource`
//! descriptors, and jumps to the kernel entry point.
//!
//! See `docs/boot-protocol.md` for the full boot contract.

#![no_std]
#![no_main]

use core::panic::PanicInfo;

// Include the hand-crafted PE/COFF header for RISC-V UEFI builds.
// On RISC-V, LLVM cannot emit PE/COFF directly, so we prepend the header via
// assembly and convert the ELF to a flat binary with llvm-objcopy.
// See boot/loader/src/arch/riscv64/header.S and boot/loader/linker/riscv64-uefi.ld.
#[cfg(target_arch = "riscv64")]
core::arch::global_asm!(include_str!("arch/riscv64/header.S"));

// UEFI entry point.
//
// The UEFI firmware calls this function after loading the bootloader image.
// `image_handle` is the handle to this application; `system_table` is the
// pointer to the UEFI system table, which provides access to firmware services.
//
// This stub simply halts. Real implementation begins in the bootloader task.
#[no_mangle]
pub extern "efiapi" fn efi_main(
    _image_handle: *const core::ffi::c_void,
    _system_table: *const core::ffi::c_void,
) -> usize
{
    loop {}
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> !
{
    loop {}
}
