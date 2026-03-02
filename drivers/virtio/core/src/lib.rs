// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// drivers/virtio/core/src/lib.rs

//! VirtIO transport and queue primitives shared by all VirtIO drivers.
//!
//! This crate provides the common VirtIO infrastructure (virtqueue management,
//! MMIO/PCI transport abstraction, device negotiation) used by individual
//! driver crates such as `virtio-blk`.
//!
//! No implementation yet — stub only.

#![no_std]
