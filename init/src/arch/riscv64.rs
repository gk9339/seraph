// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// init/src/arch/riscv64.rs

//! RISC-V serial output via 16550 UART MMIO and architecture constants.

use init_protocol::{CapType, InitInfo};

/// ELF machine type for RISC-V.
pub const EXPECTED_ELF_MACHINE: u16 = elf::EM_RISCV;

const UART_PHYS: u64 = 0x1000_0000;
const SERIAL_VA: u64 = 0x0000_0000_3000_0000;
static mut UART_BASE: u64 = 0;

/// Initialise UART serial output via MMIO.
pub fn serial_init(info: &InitInfo, _thread_cap: u32)
{
    let Some(slot) = crate::find_cap(info, CapType::MmioRegion, UART_PHYS)
    else
    {
        return;
    };
    if syscall::mmio_map(info.aspace_cap, slot, SERIAL_VA, 0).is_err()
    {
        return;
    }
    // SAFETY: single-threaded init; UART MMIO programming. SERIAL_VA is
    // mapped via mmio_map above.
    unsafe {
        UART_BASE = SERIAL_VA;
        let base = SERIAL_VA as *mut u8;
        core::ptr::write_volatile(base.add(1), 0x00);
        core::ptr::write_volatile(base.add(3), 0x80);
        core::ptr::write_volatile(base, 0x01);
        core::ptr::write_volatile(base.add(1), 0x00);
        core::ptr::write_volatile(base.add(3), 0x03);
    }
}

/// Write one byte to the UART, spinning until the transmit register is ready.
pub fn serial_write_byte(byte: u8)
{
    // SAFETY: single-threaded init; reading the static set during serial_init().
    let base = unsafe { UART_BASE };
    if base == 0
    {
        return;
    }
    let p = base as *mut u8;
    // SAFETY: UART MMIO region is mapped at UART_BASE; reading LSR is safe.
    while unsafe { core::ptr::read_volatile(p.add(5)) } & 0x20 == 0 {}
    // SAFETY: UART MMIO region is mapped at UART_BASE; writing data register.
    unsafe { core::ptr::write_volatile(p, byte) };
}
