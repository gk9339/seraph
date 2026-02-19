// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// boot/protocol/src/lib.rs

//! Boot protocol types shared between the bootloader and kernel.
//!
//! Defines the [`BootInfo`] structure and associated types that form the
//! contract between the bootloader and the kernel entry point. See
//! `docs/boot-protocol.md` for the full specification.

#![no_std]
