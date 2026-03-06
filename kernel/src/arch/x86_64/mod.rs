// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/arch/x86_64/mod.rs

//! x86-64 architecture module for the kernel.

pub mod console;
pub mod cpu;
pub mod paging;

/// Architecture name string for use in diagnostic output.
pub const ARCH_NAME: &str = "x86_64";
