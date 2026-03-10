// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/mm/mod.rs

//! Physical and virtual memory management.
//!
//! Provides the buddy frame allocator, boot-time physical memory initialization,
//! kernel page table setup, slab/size-class allocators, and the kernel heap.

pub mod address_space;
pub mod buddy;
pub mod heap;
pub mod init;
pub mod paging;
pub mod size_class;
pub mod slab;

pub use buddy::{BuddyAllocator, PAGE_SIZE};

/// Physical frame allocator, populated during Phase 2.
///
/// Stored as a crate-level static to avoid placing a ~41 KiB struct on the
/// kernel's 64 KiB boot stack. Access is single-threaded during boot; SMP
/// is not yet active.
///
/// # Safety
///
/// Accessed only from the single boot thread before SMP is enabled, or
/// through the `KernelHeap` spin-lock after Phase 4.
// SAFETY: accessed only from the single boot thread before SMP is enabled,
// or through the KernelHeap spin-lock after heap::init().
pub(crate) static mut FRAME_ALLOCATOR: BuddyAllocator = BuddyAllocator::new();
