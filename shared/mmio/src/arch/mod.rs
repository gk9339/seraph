// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// shared/mmio/src/arch/mod.rs

//! Per-architecture selection for MMIO ordering primitives.

#[cfg(target_arch = "riscv64")]
mod riscv64;
#[cfg(target_arch = "riscv64")]
pub use riscv64::{dma_to_mmio_barrier, mmio_to_dma_barrier, mmio_to_mmio_barrier};

#[cfg(target_arch = "x86_64")]
mod x86_64;
#[cfg(target_arch = "x86_64")]
pub use x86_64::{dma_to_mmio_barrier, mmio_to_dma_barrier, mmio_to_mmio_barrier};
