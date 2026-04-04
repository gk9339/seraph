// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/irq.rs

//! Kernel IRQ routing table — maps interrupt lines to signal objects.
//!
//! When a device IRQ fires, the arch-specific interrupt handler calls
//! [`dispatch_device_irq`] with the interrupt line number. This module
//! looks up the registered [`SignalState`], masks the IRQ at the controller
//! (preventing further delivery until the driver ACKs), ORs a notification
//! bit into the signal, and wakes any blocked waiter.
//!
//! Drivers register a signal via `SYS_IRQ_REGISTER` and re-enable delivery
//! via `SYS_IRQ_ACK` after handling.
//!
//! # Thread safety
//! The routing table is protected by disabling interrupts during modification.
//! `dispatch_device_irq` is only called from interrupt context (interrupts
//! disabled at entry on both x86-64 and RISC-V), so reads there are safe.
//!
//! # Modification notes
//! - To support multiple signals per IRQ line (e.g. shared interrupts): replace
//!   the single pointer with a small fixed-size list.
//! - To support SMP: the table will need a spinlock; current single-CPU
//!   assumption relies on interrupt disable being sufficient.

use crate::ipc::signal::SignalState;

// ── Routing table ─────────────────────────────────────────────────────────────

/// Maximum IRQ lines tracked. Covers x86-64 GSIs (0–255) and RISC-V PLIC
/// sources (1–127).
const MAX_IRQ: usize = 256;

/// Per-IRQ routing entry.
#[derive(Clone, Copy)]
struct IrqRoute
{
    /// Pointer to the `SignalState` to notify, or null if unregistered.
    signal: *mut SignalState,
}

// SAFETY: IrqRoute is only accessed with interrupts disabled (single-CPU).
unsafe impl Send for IrqRoute {}
unsafe impl Sync for IrqRoute {}

impl IrqRoute
{
    const fn empty() -> Self
    {
        Self {
            signal: core::ptr::null_mut(),
        }
    }
}

/// Global IRQ routing table.
///
/// Entries are set at IRQ registration time and cleared on cap deallocation.
/// Access is safe because all modifications disable interrupts, and
/// `dispatch_device_irq` is always called from interrupt context (IRQs off).
static mut IRQ_TABLE: [IrqRoute; MAX_IRQ] = {
    // const-initialise all entries to empty.
    let mut arr = [IrqRoute::empty(); MAX_IRQ];
    // const blocks cannot use loops over non-Copy types cleanly — zero-init is
    // guaranteed by BSS for a static mut, but we set explicitly for clarity.
    let mut i = 0;
    while i < MAX_IRQ
    {
        arr[i] = IrqRoute::empty();
        i += 1;
    }
    arr
};

// ── Public interface ──────────────────────────────────────────────────────────

/// Register `signal` to receive notifications for interrupt line `irq`.
///
/// Replaces any previous registration for the same line. The previous signal
/// pointer (may be null) is returned but is not dereferenced; the caller is
/// responsible for tracking object lifetimes.
///
/// # Safety
/// - `irq` must be < [`MAX_IRQ`].
/// - `signal` must be a valid, live `SignalState` pointer (or null to clear).
/// - Must be called with interrupts disabled.
#[cfg(not(test))]
pub unsafe fn register(irq: u32, signal: *mut SignalState)
{
    debug_assert!((irq as usize) < MAX_IRQ, "irq out of range");
    // SAFETY: interrupts are disabled; single-CPU; index is in bounds.
    unsafe {
        IRQ_TABLE[irq as usize].signal = signal;
    }
}

/// Clear the routing entry for `irq` (called when the Interrupt cap is freed).
///
/// # Safety
/// - `irq` must be < [`MAX_IRQ`].
/// - Must be called with interrupts disabled.
#[cfg(not(test))]
pub unsafe fn unregister(irq: u32)
{
    debug_assert!((irq as usize) < MAX_IRQ, "irq out of range");
    // SAFETY: interrupts are disabled; single-CPU; index is in bounds.
    unsafe {
        IRQ_TABLE[irq as usize].signal = core::ptr::null_mut();
    }
}

/// Clear all routing entries that point to `signal` (called when a Signal
/// object is being freed). Prevents use-after-free if a hardware IRQ fires
/// after the SignalState has been deallocated.
///
/// O(MAX_IRQ) scan; acceptable since signal deallocation is infrequent.
///
/// # Safety
/// - `signal` must be a valid (still live) `SignalState` pointer.
/// - Must be called with interrupts disabled.
#[cfg(not(test))]
pub unsafe fn unregister_signal(signal: *mut SignalState)
{
    for i in 0..MAX_IRQ
    {
        // SAFETY: interrupts are disabled; single-CPU; index is in bounds.
        unsafe {
            if core::ptr::eq(IRQ_TABLE[i].signal, signal)
            {
                IRQ_TABLE[i].signal = core::ptr::null_mut();
                // Mask the IRQ line since there's no longer a handler.
                crate::arch::current::interrupts::mask(i as u32);
            }
        }
    }
}

/// Dispatch a hardware interrupt for `irq` to its registered signal.
///
/// Called from the arch-specific device IRQ stub (x86-64: vectors 33–55;
/// RISC-V: PLIC external interrupt handler) with interrupts disabled.
///
/// Flow:
/// 1. Mask the IRQ at the controller (prevents re-entry until ACK).
/// 2. OR notification bit 0 into the registered signal.
/// 3. If a waiter was unblocked, enqueue it on the scheduler.
/// 4. Acknowledge at the controller (send EOI / PLIC complete).
///
/// If no signal is registered, the IRQ is silently dropped (masked; no ACK).
///
/// # Safety
/// Must only be called from interrupt context with interrupts disabled.
#[cfg(not(test))]
pub unsafe fn dispatch_device_irq(irq: u32)
{
    if (irq as usize) >= MAX_IRQ
    {
        return;
    }

    // SAFETY: index is bounds-checked; interrupts are disabled.
    let sig_ptr = unsafe { IRQ_TABLE[irq as usize].signal };
    if sig_ptr.is_null()
    {
        // No handler registered — mask and drop. Must still acknowledge at the
        // controller: without this, the PLIC keeps the interrupt "in-service"
        // indefinitely, blocking all future external IRQs at the same priority
        // (all sources share priority 1 by default).
        crate::arch::current::interrupts::mask(irq);
        crate::arch::current::interrupts::acknowledge(irq);
        return;
    }

    // Mask the IRQ before delivering: prevents interrupt storm if the driver
    // is slow to ACK. The driver calls SYS_IRQ_ACK to unmask.
    crate::arch::current::interrupts::mask(irq);

    // Deliver one notification bit into the signal. Bit 0 is used for
    // single-IRQ-per-signal registration (the standard case).
    // SAFETY: sig_ptr is valid and non-null; interrupts are disabled.
    let woken = unsafe { crate::ipc::signal::signal_send(sig_ptr, 1) };

    // Acknowledge at the interrupt controller (EOI / PLIC complete).
    crate::arch::current::interrupts::acknowledge(irq);

    // If signal_send woke a waiter, enqueue it so the scheduler picks it up.
    if let Some(tcb) = woken
    {
        let prio = unsafe { (*tcb).priority };
        crate::sched::scheduler_for(0).enqueue(tcb, prio);
    }
}
