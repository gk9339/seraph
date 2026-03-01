// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// boot/src/arch/x86_64/mod.rs

//! x86-64 architecture module for the bootloader.
//!
//! Exports the expected ELF machine type constant and the kernel handoff
//! function. Page table implementation is in [`paging`].

pub mod handoff;
pub mod paging;
pub mod serial;
pub use handoff::{perform_handoff, trampoline_page_range};
pub use paging::BootPageTable;

use crate::elf::EM_X86_64;

/// ELF machine type expected for x86-64 kernel binaries.
pub const EXPECTED_ELF_MACHINE: u16 = EM_X86_64;
