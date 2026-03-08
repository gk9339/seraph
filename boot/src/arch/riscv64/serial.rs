// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// boot/src/arch/riscv64/serial.rs

//! RISC-V UART backend: runtime-discovered MMIO 16550 address.
//!
//! The UART base is discovered at boot via ACPI SPCR (primary path for QEMU
//! virt with EDK2) or Device Tree (ns16550a-compatible node), with a fallback
//! to the QEMU virt default of 0x10000000.
//!
//! Call `discover_uart(st)` before `serial_init()` to update the base. If
//! discovery is skipped or fails, the hardcoded QEMU default is used silently.

use crate::uefi::{EfiSystemTable, EFI_ACPI_20_TABLE_GUID, EFI_DTB_TABLE_GUID};

/// UART MMIO base address; updated by `discover_uart` before first use.
///
/// Default: QEMU virt 0x10000000 (ns16550a at the standard QEMU address).
static mut UART_BASE_ADDR: usize = 0x1000_0000;

/// UART register offsets (byte-addressed).
const UART_TX: usize = 0; // transmit holding register
const UART_LSR: usize = 5; // line status register

/// Discover the UART MMIO base from ACPI SPCR or Device Tree.
///
/// Tries ACPI SPCR first (primary path for QEMU virt with EDK2), then DTB
/// (ns16550a-compatible node). Falls back to 0x10000000 silently if both fail.
///
/// Must be called before `serial_init()` and `serial_write_byte()`.
///
/// # Safety
/// `st` must be a valid pointer to the UEFI system table.
pub unsafe fn discover_uart(st: *mut EfiSystemTable)
{
    // Try ACPI SPCR first (EDK2 on QEMU virt provides ACPI, not DTB).
    if let Some(rsdp_ptr) =
        unsafe { crate::uefi::find_config_table(st, &EFI_ACPI_20_TABLE_GUID) }
    {
        let rsdp_addr = rsdp_ptr as u64;
        if let Some(base) = unsafe { crate::acpi::find_spcr_uart_base(rsdp_addr) }
        {
            unsafe { UART_BASE_ADDR = base as usize };
            return;
        }
    }

    // Try Device Tree (bare-metal RISC-V or non-EDK2 firmware with DTB).
    if let Some(dtb_ptr) =
        unsafe { crate::uefi::find_config_table(st, &EFI_DTB_TABLE_GUID) }
    {
        let dtb_addr = dtb_ptr as u64;
        if let Some(base) = unsafe { crate::dtb::find_uart_base(dtb_addr) }
        {
            unsafe { UART_BASE_ADDR = base as usize };
            return;
        }
    }
    // Both failed: UART_BASE_ADDR retains the QEMU default (0x10000000).
}

/// Return the currently configured UART MMIO base address.
///
/// Call after `discover_uart` for the discovered value; before it, returns the
/// default 0x10000000.
pub fn uart_base() -> usize
{
    // SAFETY: single-threaded bootloader; written only in discover_uart.
    unsafe { UART_BASE_ADDR }
}

/// Initialize the UART.
///
/// QEMU pre-initializes the UART at reset; this performs a minimal re-enable
/// (8N1, no FIFO) in case a prior stage left it in an unexpected state.
///
/// # Safety
/// Must be called at most once, after `discover_uart`. The MMIO region at
/// `UART_BASE_ADDR` must be accessible and not MMU-protected.
pub unsafe fn serial_init()
{
    let base = unsafe { UART_BASE_ADDR } as *mut u8;
    unsafe {
        // IER = 0: disable all interrupts.
        core::ptr::write_volatile(base.add(1), 0x00);
        // LCR DLAB = 1: access divisor latch.
        core::ptr::write_volatile(base.add(3), 0x80);
        // Divisor = 1 (assume clock pre-configured by QEMU).
        core::ptr::write_volatile(base.add(0), 0x01);
        core::ptr::write_volatile(base.add(1), 0x00);
        // LCR = 0x03: 8N1, DLAB = 0.
        core::ptr::write_volatile(base.add(3), 0x03);
        // FCR = 0: disable FIFO (QEMU virt does not need it).
        core::ptr::write_volatile(base.add(2), 0x00);
    }
}

/// Write a single byte to the UART, spinning until the transmit buffer is ready.
///
/// # Safety
/// `serial_init` must have been called before this function.
pub unsafe fn serial_write_byte(byte: u8)
{
    let base = unsafe { UART_BASE_ADDR } as *mut u8;

    // Spin on LSR bit 5 (THRE — Transmit Holding Register Empty).
    // SAFETY: MMIO read from LSR; UART_BASE_ADDR is valid after serial_init.
    while unsafe { core::ptr::read_volatile(base.add(UART_LSR)) } & 0x20 == 0
    {}

    // SAFETY: THRE is set; writing the byte to TX is safe.
    unsafe {
        core::ptr::write_volatile(base.add(UART_TX), byte);
    }
}
