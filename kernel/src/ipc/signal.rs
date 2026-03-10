// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/ipc/signal.rs

//! Signal IPC — bitmask-based async notification.
//!
//! A signal object holds a 64-bit bitmask. Senders OR bits into it;
//! a single waiter reads-and-clears the bitmask. If the waiter finds
//! zero bits set, it blocks until a sender delivers bits.
//!
//! # Blocking semantics
//! Only one thread may wait on a signal at a time (single-waiter invariant).
//! The waiter is woken immediately if bits are already set when it calls wait.
//!
//! # Thread safety
//! All fields are `#[cfg(not(test))]` to keep the struct out of host tests.
//! Access is serialised by the caller holding the relevant CSpace/scheduler
//! lock in kernel builds.
//!
//! # Adding multi-waiter support
//! Replace `waiter` with an intrusive queue of TCBs and wake all of them
//! on signal delivery.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::sched::thread::ThreadControlBlock;

// ── SignalObject ───────────────────────────────────────────────────────────────

/// Kernel object backing a Signal capability.
///
/// Allocated from the kernel heap via `Box`. The `KernelObjectHeader` is
/// NOT included here; it lives in `cap::object::SignalKernelObject` which
/// wraps this struct.
pub struct SignalState
{
    /// Pending signal bits. Senders OR into this; the waiter read-and-clears.
    pub bits: AtomicU64,
    /// The single thread blocked waiting for a non-zero bitmask, or null.
    ///
    /// # Safety
    /// Access to this field is serialised by the owning thread's scheduler
    /// lock. Never read/write from multiple CPUs simultaneously.
    pub waiter: *mut ThreadControlBlock,
}

// SAFETY: SignalState is accessed only under the relevant scheduler lock.
unsafe impl Send for SignalState {}
unsafe impl Sync for SignalState {}

impl SignalState
{
    /// Create a new, empty signal with no pending bits and no waiter.
    pub fn new() -> Self
    {
        Self {
            bits: AtomicU64::new(0),
            waiter: core::ptr::null_mut(),
        }
    }
}

// ── Operations ────────────────────────────────────────────────────────────────

/// Deliver `bits` to `sig`.
///
/// ORs the given bits into the signal bitmask. If a thread is currently
/// blocked waiting, wakes it (moves it to Ready state) and clears `waiter`.
///
/// Returns `Some(*mut TCB)` if a thread was woken (caller must enqueue it).
///
/// # Safety
/// Must be called with the relevant scheduler lock held (single-CPU boot
/// is safe without a lock).
#[cfg(not(test))]
pub unsafe fn signal_send(sig: *mut SignalState, bits: u64) -> Option<*mut ThreadControlBlock>
{
    // SAFETY: caller guarantees sig is valid and lock is held.
    let sig = unsafe { &mut *sig };
    sig.bits.fetch_or(bits, Ordering::Release);

    // Wake a waiter if present.
    if !sig.waiter.is_null()
    {
        let waiter = sig.waiter;
        sig.waiter = core::ptr::null_mut();
        // SAFETY: waiter is a valid TCB pointer placed here by signal_wait.
        unsafe {
            (*waiter).state = crate::sched::thread::ThreadState::Ready;
            (*waiter).ipc_state = IpcThreadState::None;
        }
        Some(waiter)
    }
    else
    {
        None
    }
}

/// Wait for at least one bit in `sig` to be set.
///
/// Reads and clears the bitmask atomically. If the result is non-zero,
/// returns `Ok(bits)` immediately (no blocking). If zero, stores `caller`
/// as the waiter, sets its state to `Blocked`, and returns `Err(())` —
/// the caller must then call the scheduler to yield the CPU.
///
/// # Safety
/// Must be called with the relevant scheduler lock held.
#[cfg(not(test))]
pub unsafe fn signal_wait(
    sig: *mut SignalState,
    caller: *mut ThreadControlBlock,
) -> Result<u64, ()>
{
    // SAFETY: caller guarantees sig is valid and lock is held.
    let sig = unsafe { &mut *sig };
    let bits = sig.bits.swap(0, Ordering::Acquire);
    if bits != 0
    {
        return Ok(bits);
    }

    // No bits available — block the caller.
    sig.waiter = caller;
    // SAFETY: caller TCB is valid.
    unsafe {
        (*caller).state = crate::sched::thread::ThreadState::Blocked;
        (*caller).ipc_state = IpcThreadState::BlockedOnSignal;
    }
    Err(())
}

// Import IpcThreadState here to avoid a circular import; it lives in thread.rs.
use crate::sched::thread::IpcThreadState;
