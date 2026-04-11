// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// devmgr/src/main.rs

//! Seraph device manager — stub.
//!
//! devmgr is responsible for platform enumeration, hardware discovery, and
//! binding drivers to devices. It is started early by init and manages the
//! lifetime of driver processes throughout the system.
//!
//! This stub exits immediately. Full implementation is deferred to Tier 3.
//!
//! See `devmgr/README.md` for the design and IPC interface.

#![no_std]
#![no_main]

// Link shared/runtime to get _start() and panic_handler.
extern crate runtime;

use process_abi::StartupInfo;

/// Device manager entry point.
///
/// TODO: real devmgr implementation (Tier 3) — ACPI/DT parsing, PCI
/// enumeration, driver matching and spawn, device registry IPC endpoint.
#[no_mangle]
extern "Rust" fn main(_startup: &StartupInfo) -> !
{
    syscall::thread_exit();
}
