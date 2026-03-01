// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// boot/src/arch/riscv64/mod.rs

//! RISC-V 64-bit architecture module for the bootloader.
//!
//! Exports the expected ELF machine type constant and the kernel handoff
//! function. Page table implementation is in [`paging`].

pub mod handoff;
pub mod paging;
pub mod serial;
pub use handoff::{perform_handoff, trampoline_page_range};
pub use paging::BootPageTable;

use crate::elf::EM_RISCV;

/// ELF machine type expected for RISC-V 64-bit kernel binaries.
pub const EXPECTED_ELF_MACHINE: u16 = EM_RISCV;
