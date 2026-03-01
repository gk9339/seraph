// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// shared/syscall/src/lib.rs

//! Raw syscall wrappers shared across Seraph userspace components.
//!
//! Provides architecture-specific inline assembly wrappers for kernel syscalls.
//! These are the lowest-level userspace primitives; higher-level abstractions
//! (capability handles, IPC channels) are built on top.
//!
//! This crate is `no_std` and has no external dependencies. Architecture
//! selection is via `#[cfg(target_arch)]`; only the active arch is compiled.
//!
//! # Module structure (planned)
//! - `arch/x86_64` — SYSCALL/SYSRET wrappers
//! - `arch/riscv64` — ECALL wrappers
//! - `cap` — capability syscall wrappers (cap_copy, cap_move, cap_revoke, etc.)
//! - `mm` — memory syscall wrappers (map, unmap, frame ops)
//! - `ipc` — IPC syscall wrappers (send, recv, call, reply)
//! - `thread` — thread syscall wrappers (create, start, exit, configure)
//!
//! This is a stub. Full implementation is deferred.

#![no_std]
