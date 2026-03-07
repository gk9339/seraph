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

/// MMIO regions that must be direct-mapped during Phase 3 page table setup.
///
/// RISC-V uses memory-mapped PLIC/CLINT, but those are already within the
/// physical RAM range that the direct map covers (< 1 GiB on QEMU virt).
/// No additional mappings needed.
pub const MMIO_DIRECT_MAP_REGIONS: &[(u64, u64)] = &[];
