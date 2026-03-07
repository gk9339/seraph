// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/arch/x86_64/mod.rs

//! x86-64 architecture module for the kernel.

pub mod console;
pub mod cpu;
pub mod gdt;
pub mod idt;
pub mod interrupts;
pub mod paging;
pub mod syscall;
pub mod timer;

/// Architecture name string for use in diagnostic output.
pub const ARCH_NAME: &str = "x86_64";

/// MMIO regions that must be direct-mapped during Phase 3 page table setup.
///
/// Each entry is `(physical_base, size_in_bytes)`. These regions are mapped
/// at `DIRECT_MAP_BASE + phys` if they fall outside the RAM direct-map range.
/// The xAPIC local APIC register block is 4 KiB at 0xFEE00000.
pub const MMIO_DIRECT_MAP_REGIONS: &[(u64, u64)] = &[(0xFEE0_0000, 0x1000)];
