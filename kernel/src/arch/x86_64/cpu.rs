// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/arch/x86_64/cpu.rs

//! x86-64 CPU control primitives.

/// Disable hardware interrupts.
///
/// # Safety
/// Changes global CPU interrupt state. Caller is responsible for re-enabling
/// interrupts when appropriate (the kernel does not enable them during early boot).
pub unsafe fn disable_interrupts()
{
    // SAFETY: caller guarantees this is called in an appropriate context.
    unsafe {
        core::arch::asm!("cli", options(nomem, nostack, preserves_flags));
    }
}

/// Disable interrupts and halt the CPU permanently.
///
/// Loops on `hlt` so that any NMI that fires during early boot does not cause
/// an uncontrolled jump; interrupts remain disabled.
pub fn halt_loop() -> !
{
    // SAFETY: cli disables interrupts; hlt is safe to execute at any privilege level.
    unsafe {
        disable_interrupts();
    }
    loop
    {
        // SAFETY: hlt puts the CPU into a low-power wait state until the next interrupt.
        // Interrupts are disabled above, so this halts permanently.
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}
