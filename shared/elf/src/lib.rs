// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// shared/elf/src/lib.rs

//! Shared ELF parsing crate for Seraph userspace components.
//!
//! Provides ELF64 parsing shared between init (which needs a minimal ELF
//! parser to load procmgr) and procmgr (which loads all other processes).
//!
//! This crate is `no_std` and has no external dependencies. It provides
//! validation, header parsing, and segment enumeration; it does not perform
//! memory allocation or I/O.
//!
//! # Module structure
//! - (planned) `header` — ELF64 header types and validation
//! - (planned) `segment` — LOAD segment iteration and permission mapping
//!
//! This is a stub. Full implementation is deferred.

#![no_std]
