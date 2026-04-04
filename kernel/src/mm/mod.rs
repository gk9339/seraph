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

use core::sync::atomic::{AtomicBool, Ordering};

/// Physical frame allocator, populated during Phase 2.
///
/// Stored as a crate-level static to avoid placing a ~41 KiB struct on the
/// kernel's 64 KiB boot stack. Access is single-threaded during boot; SMP
/// is not yet active.
///
/// # Safety
///
/// Accessed only from the single boot thread before SMP is enabled, or
/// through `with_frame_allocator` (direct allocation) or the `KernelHeap`
/// spin-lock (heap allocation) after Phase 4.
///
/// # SMP note
///
/// `with_frame_allocator` and the heap's `KernelHeap::with_lock` each protect
/// `FRAME_ALLOCATOR` with separate spinlocks. On a single CPU this is safe:
/// the kernel is not re-entered while a spinlock is held. For SMP (WSMP)
/// the heap's `with_lock` must also acquire `FRAME_ALLOC_LOCK` before
/// accessing `FRAME_ALLOCATOR`.
// SAFETY: accessed only from the single boot thread before SMP is enabled,
// or through with_frame_allocator / KernelHeap spin-lock after Phase 4.
pub(crate) static mut FRAME_ALLOCATOR: BuddyAllocator = BuddyAllocator::new();

/// Spin-lock protecting direct (non-heap) access to `FRAME_ALLOCATOR`.
///
/// Acquired by `with_frame_allocator`. The heap's own lock independently
/// protects `FRAME_ALLOCATOR` for slab/size-class paths; see the SMP note
/// on `FRAME_ALLOCATOR` above.
static FRAME_ALLOC_LOCK: AtomicBool = AtomicBool::new(false);

/// Call `f` with exclusive access to the frame allocator.
///
/// Acquires `FRAME_ALLOC_LOCK`, grants `f` a mutable reference to
/// `FRAME_ALLOCATOR`, then releases the lock. Use this for direct frame
/// allocation (kernel stack allocation, page table frame allocation) from
/// syscall handlers or runtime kernel code.
///
/// Do NOT call `Box::new` (heap allocation) inside `f` — the heap has its
/// own lock and calls this allocator internally, which is safe because they
/// are separate critical sections on a single CPU.
///
/// # Safety
///
/// Must be called after Phase 2 (frame allocator populated). Must not be
/// called before the direct physical map is active (Phase 3).
#[cfg(not(test))]
pub(crate) fn with_frame_allocator<F, R>(f: F) -> R
where
    F: FnOnce(&mut BuddyAllocator) -> R,
{
    // Spin until we acquire the lock.
    let mut spins = 0u64;
    while FRAME_ALLOC_LOCK
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        spins += 1;
        if spins > 500_000 {
            crate::kprintln!("[frame_alloc] DEADLOCK after {}k spins", spins / 1000);
            loop { core::hint::spin_loop(); }
        }
        core::hint::spin_loop();
    }

    // SAFETY: we hold FRAME_ALLOC_LOCK; no concurrent with_frame_allocator call.
    let result = f(unsafe { &mut *core::ptr::addr_of_mut!(FRAME_ALLOCATOR) });

    FRAME_ALLOC_LOCK.store(false, Ordering::Release);
    result
}
