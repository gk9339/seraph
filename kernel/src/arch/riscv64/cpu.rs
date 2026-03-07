// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/arch/riscv64/cpu.rs

//! RISC-V 64-bit CPU control primitives.
//!
//! # Phase 5 additions
//! - `halt_until_interrupt` — execute `wfi` with SIE enabled so the timer fires.
//! - `current_id` — returns 0 (BSP only; real hart ID deferred to SMP phase).

// ── Phase 5 additions ─────────────────────────────────────────────────────────

/// Suspend the hart until the next interrupt fires, then return.
///
/// Unlike `halt_loop`, this requires interrupts to be enabled (via
/// `sstatus.SIE`) so the supervisor timer can wake the hart.
/// Interrupts remain enabled after `wfi` returns.
pub fn halt_until_interrupt()
{
    // SAFETY: wfi is a hint; the CPU suspends until an enabled interrupt arrives.
    unsafe {
        core::arch::asm!("wfi", options(nostack, nomem));
    }
}

/// Return the current hart ID.
///
/// Phase 5: only the BSP is running; returns 0.
/// Future: read `mhartid` via SBI `sbi_get_marchid` or from the boot-info
/// structure when SMP is brought up.
pub fn current_id() -> u32
{
    0
}

// ── Interrupt control ─────────────────────────────────────────────────────────

/// Disable supervisor-mode interrupts via sstatus.SIE.
///
/// # Safety
/// Changes global CPU interrupt state. Caller is responsible for managing
/// interrupt state across the transition.
pub unsafe fn disable_interrupts()
{
    // SAFETY: csrci clears the SIE bit (bit 1) in sstatus.
    // Caller guarantees this is called in supervisor mode.
    unsafe {
        core::arch::asm!("csrci sstatus, 0x2", options(nomem, nostack));
    }
}

/// Disable interrupts and halt the CPU permanently using `wfi`.
///
/// `wfi` (wait-for-interrupt) suspends the hart until an interrupt arrives.
/// With SIE cleared the hart cannot actually handle the interrupt, so it
/// re-executes `wfi` immediately — achieving an effective halt without a
/// busy spin.
pub fn halt_loop() -> !
{
    // SAFETY: disabling interrupts before wfi is required for a safe permanent halt.
    unsafe {
        disable_interrupts();
    }
    loop
    {
        // SAFETY: wfi is a hint that the hart may be suspended; safe at any privilege level.
        unsafe {
            core::arch::asm!("wfi", options(nomem, nostack));
        }
    }
}
