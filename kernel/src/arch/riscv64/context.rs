// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/arch/riscv64/context.rs

//! RISC-V 64-bit thread context management.
//!
//! `SavedState` holds the kernel-mode callee-saved register set for one thread.
//! `new_state` constructs the initial state for a new thread.
//!
//! `switch` and `return_to_user` are stubs that call `fatal()` — real
//! implementations are deferred to Phase 9/10.

use crate::fatal;

// ── SavedState ────────────────────────────────────────────────────────────────

/// Kernel-mode callee-saved register state for one thread.
///
/// On each context switch only this minimal set is saved/restored (see
/// `docs/scheduler.md` — "What Gets Saved and Restored"). The full user
/// register file (a0–a7, t0–t6, etc.) lives in the thread's trap frame.
///
/// Layout matches the push order used by the assembly switch stub (Phase 9).
#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct SavedState
{
    /// Stack pointer.
    pub sp: u64,
    /// Return address — where execution resumes after `switch` returns.
    pub ra: u64,
    /// Callee-saved general-purpose registers (s0/fp = frame pointer).
    pub s0: u64,
    pub s1: u64,
    pub s2: u64,
    pub s3: u64,
    pub s4: u64,
    pub s5: u64,
    pub s6: u64,
    pub s7: u64,
    pub s8: u64,
    pub s9: u64,
    pub s10: u64,
    pub s11: u64,
    /// Thread pointer — used for per-thread TLS pointer.
    pub tp: u64,
    /// Supervisor status register snapshot.
    pub sstatus: u64,
    /// First argument register (a0); holds the entry argument for new threads.
    ///
    /// Not a callee-saved register, but stored here so the Phase 9 switch stub
    /// can deliver the argument when first entering a new thread.
    ///
    /// TODO Phase 9: deliver via a0 in the switch stub; remove from SavedState
    /// once the argument-passing convention is settled.
    pub a0: u64,
}

// ── new_state ─────────────────────────────────────────────────────────────────

/// Construct the initial [`SavedState`] for a new thread.
///
/// `entry`     — virtual address of the thread's entry function.
/// `stack_top` — top of the thread's kernel stack (stack grows down; sp starts here).
/// `arg`       — first argument passed in `a0`.
/// `is_user`   — if false (kernel thread), `sstatus.SPP` is left at 0 (supervisor);
///               no user-mode bits needed until Phase 9 `return_to_user`.
///
/// # TODO Phase 9
/// For user threads set `sstatus.SPP = 0` (user) and `sstatus.SPIE = 1`
/// (enable interrupts on `sret`). The real `switch` / `return_to_user` stubs
/// in Phase 9 use `sepc` + `sret`; adjust `ra` usage accordingly.
pub fn new_state(entry: u64, stack_top: u64, arg: u64, is_user: bool) -> SavedState
{
    // sstatus.SIE (bit 1) — supervisor interrupt enable while in S-mode.
    // Set for kernel threads so the timer can fire once the thread runs.
    let sstatus: u64 = 1 << 1; // SIE
    let _ = is_user; // user-mode sstatus bits handled in Phase 9 return_to_user.

    SavedState {
        ra: entry,
        sp: stack_top,
        a0: arg,
        sstatus,
        ..SavedState::default()
    }
}

// ── switch ────────────────────────────────────────────────────────────────────

/// Save `current`'s registers and restore `next`'s.
///
/// # Safety
/// Both pointers must point to valid, aligned `SavedState` values.
///
/// # TODO Phase 9
/// Replace this stub with real save/restore assembly. The switch must:
/// 1. Save s0–s11, ra, sp, tp, sstatus into `*current`.
/// 2. Restore the same fields from `*next`.
/// 3. Return (ret), which jumps to `next.ra`.
/// 4. Update sscratch with `next_tcb.kernel_stack_top` for trap entry.
#[allow(unused_variables)]
pub unsafe fn switch(current: *mut SavedState, next: *const SavedState)
{
    // TODO Phase 9: implement register save/restore.
    fatal("context::switch not yet implemented (Phase 9)");
}

// ── return_to_user ────────────────────────────────────────────────────────────

/// Restore full user register state and return to user mode via `sret`.
///
/// # Safety
/// `state` must hold a valid user-mode register snapshot.
///
/// # TODO Phase 9
/// Replace this stub with assembly that:
/// 1. Restores a0–a7, t0–t6, s0–s11, ra, sp, tp from the trap frame.
/// 2. Loads `sepc` with the user program counter.
/// 3. Sets `sstatus.SPP = 0` (return to user) and `sstatus.SPIE = 1`.
/// 4. Executes `sret`.
#[allow(unused_variables)]
pub unsafe fn return_to_user(state: &SavedState) -> !
{
    // TODO Phase 9: implement sret path.
    fatal("context::return_to_user not yet implemented (Phase 9)");
}
