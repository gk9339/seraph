// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/arch/x86_64/context.rs

//! x86-64 thread context management.
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
/// `docs/scheduler.md` — "What Gets Saved and Restored"). Caller-saved
/// registers are the calling code's responsibility per the System V AMD64 ABI.
///
/// Layout matches the push order used by the assembly switch stub (Phase 9).
#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct SavedState
{
    /// Instruction pointer — where execution resumes.
    pub rip: u64,
    /// Stack pointer.
    pub rsp: u64,
    /// Callee-saved general-purpose registers.
    pub rbx: u64,
    pub rbp: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    /// FS.base MSR — per-thread TLS pointer.
    pub fs_base: u64,
    /// RFLAGS at the moment of the switch.
    pub rflags: u64,
}

// ── new_state ─────────────────────────────────────────────────────────────────

/// Construct the initial [`SavedState`] for a new thread.
///
/// `entry`    — virtual address of the thread's entry function.
/// `stack_top` — top of the thread's kernel stack (stack grows down; RSP starts here).
/// `arg`      — first argument passed in `rdi` (System V AMD64 ABI).
/// `is_user`  — if false (kernel thread), RFLAGS has IF set only; no user-mode bits.
///
/// The `rdi` register is caller-saved, so it is not included in `SavedState`.
/// The real switch stub will set up `rdi` separately (Phase 9). For now the
/// field is stored in `rbx` as a convenience so the initialisation record is
/// self-contained; the Phase 9 assembly will use it from there.
///
/// # TODO Phase 9
/// When the real `switch` assembly is written, revisit how `rdi` (the arg) is
/// delivered to the new thread's entry function. The typical approach is to
/// push `arg` as the first item on the new stack and have the switch stub pop
/// it into `rdi` before calling the entry point.
pub fn new_state(entry: u64, stack_top: u64, arg: u64, is_user: bool) -> SavedState
{
    // IF = bit 9 of RFLAGS. Set for both kernel and user threads so that
    // timer interrupts can fire once the thread starts. For kernel threads
    // (is_user = false) no other user-mode-specific RFLAGS bits are needed.
    let rflags: u64 = 0x200; // IF only
    let _ = is_user; // user-mode RFLAGS bits added in Phase 9 return_to_user path.

    SavedState {
        rip: entry,
        rsp: stack_top,
        // Temporarily stash arg in rbx so it is not lost at TCB creation time.
        // TODO Phase 9: deliver arg via rdi in the switch stub.
        rbx: arg,
        rflags,
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
/// 1. Save rbx, rbp, r12–r15, rsp, fs_base, rflags into `*current`.
/// 2. Restore the same fields from `*next`.
/// 3. Jump to `next.rip` (via `ret` after pushing rip onto the stack, or
///    via an explicit `jmp`/`call` convention agreed with new_state).
/// 4. Update TSS RSP0 for the next thread (call `gdt::set_rsp0`).
#[allow(unused_variables)]
pub unsafe fn switch(current: *mut SavedState, next: *const SavedState)
{
    // TODO Phase 9: implement register save/restore.
    fatal("context::switch not yet implemented (Phase 9)");
}

// ── return_to_user ────────────────────────────────────────────────────────────

/// Restore full user register state and return to user mode.
///
/// # Safety
/// `state` must hold a valid user-mode register snapshot.
///
/// # TODO Phase 9
/// Replace this stub with `sysretq` / `iretq` assembly that restores the full
/// user register file (rax, rcx, rdx, rsi, rdi, r8–r11 in addition to the
/// callee-saved set) and transitions to ring 3.
#[allow(unused_variables)]
pub unsafe fn return_to_user(state: &SavedState) -> !
{
    // TODO Phase 9: implement sysretq/iretq path.
    fatal("context::return_to_user not yet implemented (Phase 9)");
}
