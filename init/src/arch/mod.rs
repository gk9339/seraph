// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// init/src/arch/mod.rs

//! Architecture-specific constants and serial output.

#[cfg(target_arch = "x86_64")]
mod x86_64;
#[cfg(target_arch = "x86_64")]
pub use x86_64::{serial_init, serial_write_byte, EXPECTED_ELF_MACHINE};

#[cfg(target_arch = "riscv64")]
mod riscv64;
#[cfg(target_arch = "riscv64")]
pub use riscv64::{serial_init, serial_write_byte, EXPECTED_ELF_MACHINE};
