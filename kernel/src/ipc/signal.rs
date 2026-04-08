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
    /// Opaque pointer to the `WaitSetState` this signal is registered with,
    /// or null if not in any wait set. Type-erased to avoid a circular import.
    /// Cast to `*mut WaitSetState` only inside `wait_set.rs`.
    pub wait_set: *mut u8,
    /// Index of this signal's entry in `WaitSetState::members`.
    pub wait_set_member_idx: u8,
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
            wait_set: core::ptr::null_mut(),
            wait_set_member_idx: 0,
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
    // If a waiter is present, atomically swap the bits out so we can deliver
    // the exact value to the waiter rather than leaving it to read-and-clear.
    if sig.waiter.is_null()
    {
        sig.bits.fetch_or(bits, Ordering::Release);
        // Notify any registered wait set that this signal now has bits pending.
        if !sig.wait_set.is_null()
        {
            // SAFETY: wait_set is a valid *mut WaitSetState registered by
            // sys_wait_set_add and cleared on removal or wait_set_drop.
            unsafe { crate::ipc::wait_set::waitset_notify(sig.wait_set, sig.wait_set_member_idx) };
        }
        None
    }
    else
    {
        // OR our bits in, then swap the whole bitmask to zero so the waiter
        // gets exactly what was pending (including bits set before this call).
        sig.bits.fetch_or(bits, Ordering::AcqRel);
        let delivered = sig.bits.swap(0, Ordering::AcqRel);

        let waiter = sig.waiter;
        sig.waiter = core::ptr::null_mut();
        // SAFETY: waiter is a valid TCB pointer placed here by signal_wait.
        unsafe {
            (*waiter).wakeup_value = delivered;
            (*waiter).state = crate::sched::thread::ThreadState::Ready;
            (*waiter).ipc_state = IpcThreadState::None;
            (*waiter).blocked_on_object = core::ptr::null_mut();
        }
        Some(waiter)
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
pub unsafe fn signal_wait(sig: *mut SignalState, caller: *mut ThreadControlBlock)
    -> Result<u64, ()>
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
        (*caller).blocked_on_object = core::ptr::addr_of_mut!(*sig).cast::<u8>();
    }
    Err(())
}

// Import IpcThreadState here to avoid a circular import; it lives in thread.rs.
use crate::sched::thread::IpcThreadState;

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests
{
    use super::*;
    use core::sync::atomic::Ordering;

    #[test]
    fn new_state_is_zeroed()
    {
        let s = SignalState::new();
        assert_eq!(s.bits.load(Ordering::Relaxed), 0);
        assert!(s.waiter.is_null());
        assert!(s.wait_set.is_null());
        assert_eq!(s.wait_set_member_idx, 0);
    }

    #[test]
    fn bits_fetch_or_accumulates()
    {
        let s = SignalState::new();
        s.bits.fetch_or(0x0F, Ordering::Relaxed);
        s.bits.fetch_or(0xF0, Ordering::Relaxed);
        assert_eq!(s.bits.load(Ordering::Relaxed), 0xFF);
    }

    #[test]
    fn bits_swap_clears_and_returns_value()
    {
        let s = SignalState::new();
        s.bits.fetch_or(0xDEAD_BEEF, Ordering::Relaxed);
        let got = s.bits.swap(0, Ordering::Relaxed);
        assert_eq!(got, 0xDEAD_BEEF);
        assert_eq!(s.bits.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn bits_independent_after_swap()
    {
        // After a swap-to-zero, subsequent ORs start fresh.
        let s = SignalState::new();
        s.bits.fetch_or(0xFF, Ordering::Relaxed);
        s.bits.swap(0, Ordering::Relaxed);
        s.bits.fetch_or(0x01, Ordering::Relaxed);
        assert_eq!(s.bits.load(Ordering::Relaxed), 0x01);
    }

    #[test]
    fn multiple_fetch_or_accumulates_all_bits()
    {
        // Four non-overlapping ORs must accumulate into a single value.
        let s = SignalState::new();
        s.bits.fetch_or(0x1, Ordering::Relaxed);
        s.bits.fetch_or(0x2, Ordering::Relaxed);
        s.bits.fetch_or(0x4, Ordering::Relaxed);
        s.bits.fetch_or(0x8, Ordering::Relaxed);
        let result = s.bits.swap(0, Ordering::Relaxed);
        assert_eq!(result, 0xF, "all four bit groups must be accumulated");
    }

    #[test]
    fn swap_after_multiple_ors_leaves_state_zero()
    {
        // swap-to-zero clears all accumulated bits; subsequent ORs start fresh.
        let s = SignalState::new();
        s.bits.fetch_or(0xDEAD, Ordering::Relaxed);
        s.bits.fetch_or(0xBEEF, Ordering::Relaxed);
        let before = s.bits.swap(0, Ordering::Relaxed);
        assert_eq!(
            before,
            0xDEAD | 0xBEEF,
            "swap must return OR of all previous fetches"
        );
        assert_eq!(
            s.bits.load(Ordering::Relaxed),
            0,
            "state must be zero after swap"
        );
        // New OR starts from zero.
        s.bits.fetch_or(0x1234, Ordering::Relaxed);
        assert_eq!(s.bits.load(Ordering::Relaxed), 0x1234);
    }
}
