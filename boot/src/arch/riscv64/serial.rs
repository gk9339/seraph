// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// boot/src/arch/riscv64/serial.rs

//! QEMU virt UART backend for RISC-V (MMIO 16550 at 0x10000000).
//!
//! QEMU's virt machine pre-initializes the UART; this module performs a
//! minimal reset and provides byte-level write access.
//!
//! TODO: Replace the hardcoded MMIO base with a value from the Device Tree.
//! QEMU virt always places the UART at 0x10000000, but real RISC-V boards
//! may differ. A proper implementation should parse the DTB provided by
//! firmware and look up the "ns16550a" compatible node.

/// QEMU virt UART MMIO base address.
const UART_BASE: usize = 0x1000_0000;

/// UART register offsets (byte-addressed).
const UART_TX: usize = 0; // transmit holding register
const UART_LSR: usize = 5; // line status register

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
    let base = UART_BASE as *mut u8;
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
    let base = UART_BASE as *mut u8;

    // Spin on LSR bit 5 (THRE — Transmit Holding Register Empty).
    // SAFETY: MMIO read from LSR; UART_BASE is valid after serial_init.
    while unsafe { core::ptr::read_volatile(base.add(UART_LSR)) } & 0x20 == 0
    {}

    // SAFETY: THRE is set; writing the byte to TX is safe.
    unsafe {
        core::ptr::write_volatile(base.add(UART_TX), byte);
    }
}
