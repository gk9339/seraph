// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/arch/riscv64/mod.rs

//! RISC-V 64-bit architecture module for the kernel.

pub mod console;
pub mod cpu;

/// Architecture name string for use in diagnostic output.
pub const ARCH_NAME: &str = "riscv64";
