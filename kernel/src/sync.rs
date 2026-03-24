// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/sync.rs

//! Kernel synchronisation primitives.
//!
//! Provides a ticket spinlock that disables interrupts on lock acquisition and
//! restores them on drop, preventing timer-driven deadlock when the scheduler
//! lock is held.
//!
//! # Adding new primitives
//! Place reader-writer locks, semaphores, etc. as additional `pub mod` entries
//! in this file.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicU32, Ordering};

// ── Spinlock ──────────────────────────────────────────────────────────────────

/// A ticket spinlock that disables interrupts while held.
///
/// Uses two `AtomicU32` counters: `next_ticket` (next ticket to issue) and
/// `now_serving` (ticket currently allowed to hold the lock). Fairness is
/// guaranteed: waiters are served in acquisition order.
///
/// Interrupts are disabled on `lock()` and restored to their prior state on
/// `SpinlockGuard` drop, preventing deadlock from timer preemption.
pub struct Spinlock<T>
{
    next_ticket: AtomicU32,
    now_serving: AtomicU32,
    data: UnsafeCell<T>,
}

// SAFETY: The spinlock serialises access to T and disables interrupts while
// held, so T can be sent across thread/CPU boundaries safely.
unsafe impl<T: Send> Send for Spinlock<T> {}
unsafe impl<T: Send> Sync for Spinlock<T> {}

impl<T> Spinlock<T>
{
    /// Create a new, unlocked spinlock wrapping `value`.
    pub const fn new(value: T) -> Self
    {
        Self {
            next_ticket: AtomicU32::new(0),
            now_serving: AtomicU32::new(0),
            data: UnsafeCell::new(value),
        }
    }

    /// Acquire the lock without RAII, returning the saved interrupt flags.
    ///
    /// The caller **must** release the lock with [`unlock_raw`][Self::unlock_raw]
    /// after finishing the critical section. Use [`lock`][Self::lock] (RAII) in
    /// preference wherever possible; `lock_raw` exists for cases where holding a
    /// borrow to `Self` for the duration of the guard would create an aliasing
    /// conflict (e.g., when other fields of the same struct must be mutated while
    /// the lock is held).
    ///
    /// # Safety
    /// The returned `u64` must be passed verbatim to `unlock_raw`. Failure to
    /// release the lock will deadlock the CPU.
    pub unsafe fn lock_raw(&self) -> u64
    {
        let saved = save_and_disable_interrupts();
        let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);
        while self.now_serving.load(Ordering::Acquire) != ticket
        {
            core::hint::spin_loop();
        }
        saved
    }

    /// Release a lock acquired with [`lock_raw`][Self::lock_raw].
    ///
    /// Advances the now-serving counter and restores the interrupt state saved
    /// by the matching `lock_raw` call.
    ///
    /// # Safety
    /// `saved_flags` must be the value returned by the corresponding `lock_raw`.
    pub unsafe fn unlock_raw(&self, saved_flags: u64)
    {
        self.now_serving.fetch_add(1, Ordering::Release);
        restore_interrupts(saved_flags);
    }

    /// Acquire the lock.
    ///
    /// Disables interrupts, takes a ticket, then spins until that ticket is
    /// served. Returns a [`SpinlockGuard`] that re-enables interrupts on drop.
    pub fn lock(&self) -> SpinlockGuard<'_, T>
    {
        // Disable interrupts before taking a ticket to prevent a timer
        // interrupt from arriving after we check the lock but before we
        // disable interrupts, which would leave interrupts disabled
        // unexpectedly.
        let saved = save_and_disable_interrupts();

        let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);
        while self.now_serving.load(Ordering::Acquire) != ticket
        {
            core::hint::spin_loop();
        }

        SpinlockGuard {
            lock: self,
            saved_flags: saved,
        }
    }
}

/// RAII guard returned by [`Spinlock::lock`].
///
/// Releases the lock and restores interrupt state when dropped.
pub struct SpinlockGuard<'a, T>
{
    lock: &'a Spinlock<T>,
    saved_flags: u64,
}

impl<T> Drop for SpinlockGuard<'_, T>
{
    fn drop(&mut self)
    {
        // Release the lock by advancing now_serving, then restore interrupts.
        self.lock.now_serving.fetch_add(1, Ordering::Release);
        restore_interrupts(self.saved_flags);
    }
}

impl<T> Deref for SpinlockGuard<'_, T>
{
    type Target = T;

    fn deref(&self) -> &T
    {
        // SAFETY: we hold the lock; no other holder can exist.
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> DerefMut for SpinlockGuard<'_, T>
{
    fn deref_mut(&mut self) -> &mut T
    {
        // SAFETY: we hold the lock exclusively.
        unsafe { &mut *self.lock.data.get() }
    }
}

// ── Interrupt save/restore helpers ────────────────────────────────────────────

/// Save the current interrupt-enable flag and disable interrupts.
/// Returns an opaque value to pass to [`restore_interrupts`].
///
/// Delegates to the arch-specific implementation in `cpu`.
#[cfg(not(test))]
fn save_and_disable_interrupts() -> u64
{
    // SAFETY: called only in kernel context (ring 0 / S-mode).
    unsafe { crate::arch::current::cpu::save_and_disable_interrupts() }
}

/// Restore interrupt state saved by [`save_and_disable_interrupts`].
#[cfg(not(test))]
fn restore_interrupts(saved: u64)
{
    // SAFETY: `saved` came from a matching `save_and_disable_interrupts` call.
    unsafe { crate::arch::current::cpu::restore_interrupts(saved); }
}

// In test builds, interrupts are not a concern; no-op stubs allow the rest of
// the module to compile and run on the host.
#[cfg(test)]
fn save_and_disable_interrupts() -> u64
{
    0
}

#[cfg(test)]
fn restore_interrupts(_saved: u64) {}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn lock_unlock()
    {
        let sl = Spinlock::new(42u32);
        {
            let mut g = sl.lock();
            assert_eq!(*g, 42);
            *g = 99;
        }
        let g = sl.lock();
        assert_eq!(*g, 99);
    }

    #[test]
    fn sequential_acquisition()
    {
        let sl = Spinlock::new(0u32);
        for i in 0..10u32
        {
            let mut g = sl.lock();
            *g = i;
        }
        let g = sl.lock();
        assert_eq!(*g, 9);
    }
}
