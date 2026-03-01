// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

//! Syscall ABI definitions.
//!
//! This crate defines the binary interface between userspace and the kernel
//! for system calls. It is the single source of truth for:
//!
//! - Syscall numbers
//! - Argument layout and register conventions
//! - Return codes and error values
//! - `#[repr(C)]` types transferred across the boundary
//!
//! # Usage
//!
//! - The kernel's syscall dispatch layer imports this crate to decode incoming
//!   syscall arguments and construct return values.
//! - Userspace wrappers in `shared/syscall` import this crate for constants and
//!   types, then invoke the kernel using inline assembly.
//!
//! # Rules
//!
//! - No std; this crate must build in a no_std environment.
//! - No inline assembly; assembly belongs in `shared/syscall`.
//! - All types must be `#[repr(C)]` and have stable layout.
//! - No dependencies outside the Rust standard library primitives (core only).

#![no_std]
