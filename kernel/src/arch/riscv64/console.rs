// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/arch/riscv64/console.rs

//! QEMU virt UART backend for RISC-V (MMIO 16550 at 0x10000000).
//!
//! QEMU's virt machine pre-initializes the UART; this module performs a
//! minimal reset and provides byte-level write access.
//!
//! After Phase 3 activates the kernel's page tables, the UART is no longer
//! accessible at its physical address (0x10000000); call [`rebase_serial`]
//! to switch to the direct-map virtual address.
//!
//! # UART base address
//! The physical base is currently hardcoded to the QEMU virt default.
//! The bootloader discovers the actual base via ACPI SPCR (primary path with
//! EDK2) or DTB ns16550a node, but does not yet pass it to the kernel — there
//! is no dedicated field in `BootInfo` for this. On QEMU virt both resolve to
//! 0x10000000, so the hardcode is correct for the supported target.
//!
//! TODO: Pass the discovered UART base through `BootInfo` to the kernel.
//! Steps: add `uart_phys_base: u64` to `BootInfo` (bump protocol version),
//! set it from `arch::current::uart_mmio_region()` in the bootloader's
//! Step 9, and replace `UART_PHYS_BASE` here with `info.uart_phys_base`.

/// Physical address of the QEMU virt UART MMIO region.
///
/// Exported so callers can compute `DIRECT_MAP_BASE + UART_PHYS_BASE` and
/// pass it to [`rebase_serial`] after the page table switch.
pub const UART_PHYS_BASE: u64 = 0x1000_0000;

/// UART register offsets (byte-addressed).
const UART_TX: usize = 0; // transmit holding register
const UART_LSR: usize = 5; // line status register

/// Current UART virtual base address.
///
/// Initialized to the physical address (identity-mapped by the bootloader).
/// Updated by [`rebase_serial`] after Phase 3 switches to the direct map.
/// Single-threaded early boot: no locking required.
// SAFETY: accessed only from the single kernel boot thread.
static mut UART_BASE: u64 = UART_PHYS_BASE;

/// Switch the UART accessor to a new virtual base address.
///
/// Call this after Phase 3 activates the kernel's page tables, passing
/// `phys_to_virt(UART_PHYS_BASE)` so subsequent serial output uses the
/// direct-map address instead of the now-unmapped physical address.
///
/// # Safety
/// Must be called from the single kernel boot thread after the direct
/// physical map is active (i.e. after `activate` returns successfully).
pub unsafe fn rebase_serial(new_base: u64)
{
    // SAFETY: single-threaded boot; no concurrent access.
    unsafe { UART_BASE = new_base };
}

/// Initialize the QEMU virt UART.
///
/// QEMU pre-initializes the UART at reset; this performs a minimal re-enable
/// (8N1, no FIFO) in case a prior stage left it in an unexpected state.
///
/// # Safety
/// Caller must ensure this is called at most once and that the MMIO region
/// at `UART_BASE` is accessible and not protected by the MMU.
pub unsafe fn serial_init()
{
    // SAFETY: UART_BASE is valid (identity-mapped by bootloader at init time).
    let base = unsafe { UART_BASE } as *mut u8;
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
    // SAFETY: UART_BASE is valid (set by serial_init or rebase_serial).
    let base = unsafe { UART_BASE } as *mut u8;

    // Spin on LSR bit 5 (THRE — Transmit Holding Register Empty).
    while unsafe { core::ptr::read_volatile(base.add(UART_LSR)) } & 0x20 == 0
    {}

    unsafe {
        core::ptr::write_volatile(base.add(UART_TX), byte);
    }
}
