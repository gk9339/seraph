// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/lib.rs

//! Seraph microkernel — library target.
//!
//! This target is compiled for the **host** during `cargo test`, enabling
//! unit tests on pure algorithmic modules (buddy allocator, capability tree,
//! scheduler run queues, etc.) without requiring QEMU.
//!
//! The `no_std` attribute is suppressed during test builds so that the
//! standard test harness can link against `std`. Hardware-dependent code
//! lives behind the `arch` trait boundaries and is mocked in tests.

#![cfg_attr(not(test), no_std)]
