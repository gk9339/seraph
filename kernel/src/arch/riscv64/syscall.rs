// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/arch/riscv64/syscall.rs

//! RISC-V ecall (syscall) handling stub for Phase 5.
//!
//! On RISC-V, userspace system calls are issued with the `ecall` instruction.
//! The trap is routed through the `stvec` handler installed by `interrupts::init`,
//! which calls `syscall_stub` on scause = 8 (U-mode ecall).
//!
//! There is no separate MSR-equivalent to configure: the mechanism is
//! inherent in the trap infrastructure.
//!
//! # Modification notes
//! - Phase 9: replace `syscall_stub` with a proper dispatch table.
//!   The TrapFrame passed from `trap_dispatch` will carry the syscall number
//!   in a0 and arguments in a1-a5; results go back in a0.

/// Install the ecall handling mechanism.
///
/// No-op on RISC-V: ecall is automatically routed to `stvec` (installed by
/// `interrupts::init`). This function exists for symmetry with the x86-64
/// interface so `main.rs` can call `arch::current::syscall::init()` uniformly.
///
/// # Safety
/// No-op; always safe to call.
pub unsafe fn init() {}

/// Ecall stub: called by `trap_dispatch` when scause = 8 (U-mode ecall).
///
/// Phase 5: no userspace exists; print and halt.
/// Phase 9: replace with real syscall dispatch.
pub fn syscall_stub()
{
    crate::kprintln!("ecall from userspace");
    crate::fatal("syscall: not implemented");
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests
{
    // init() is a no-op and trivially correct; no tests needed.
    // syscall_stub() calls fatal() which is not testable in isolation.
}
