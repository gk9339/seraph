// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/arch/riscv64/mod.rs

//! RISC-V 64-bit architecture module for the kernel.

pub mod console;
pub mod cpu;
pub mod interrupts;
pub mod paging;
pub mod syscall;
pub mod timer;

/// Architecture name string for use in diagnostic output.
pub const ARCH_NAME: &str = "riscv64";

/// Maximum valid PLIC source number on the QEMU virt platform.
/// Sources 1–127 are usable; source 0 is reserved by the PLIC spec.
pub const MAX_IRQ_ID: u32 = 127;

/// Minimum valid PLIC source number. PLIC source 0 is reserved and never
/// wired to a real device.
pub const MIN_IRQ_ID: u32 = 1;

/// RISC-V has no I/O port space; IoPortRange resources are silently skipped.
pub const HAS_IO_PORTS: bool = false;

/// MMIO regions that must be direct-mapped during Phase 3 page table setup.
///
/// RISC-V uses memory-mapped PLIC/CLINT, but those are already within the
/// physical RAM range that the direct map covers (< 1 GiB on QEMU virt).
/// No additional mappings needed.
pub const MMIO_DIRECT_MAP_REGIONS: &[(u64, u64)] = &[];
