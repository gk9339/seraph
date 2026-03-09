// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/sched/thread.rs

//! Thread Control Block (TCB) definition.
//!
//! Each kernel thread has exactly one TCB. TCBs are heap-allocated via `Box`
//! for Phase 8 idle threads; later phases will use the `tcb_cache` slab for
//! user threads (see `kernel/docs/scheduler.md`).
//!
//! # Adding fields
//! When Phase 9 adds IPC: populate the `reply_cap_slot`, `pending_send`,
//! `wakeup_value`, `wakeup_token`, and `ipc_wait_next` fields.
//! When Phase 10 adds SMP: populate `address_space` and `cspace`.

use crate::arch::current::context::SavedState;

// ── ThreadState ───────────────────────────────────────────────────────────────

/// Lifecycle state of a thread.
///
/// Transitions:
/// ```text
/// Created ──(SYS_THREAD_START)──► Ready ──(scheduled)──► Running
///                                   ▲                       │
///                                   └──── (preempt/yield) ──┘
///                                   │
///                               (IPC block, etc.)
///                                   │
///                                 Blocked
///                                   │
///                               (wakeup)
///                                   ▼
///                                 Ready
/// Running ──(SYS_THREAD_STOP)──► Stopped
/// Running ──(SYS_THREAD_EXIT)──► Exited  (TCB freed)
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState
{
    /// Allocated but not yet started.
    Created,
    /// Runnable; in a CPU run queue.
    Ready,
    /// Currently executing on a CPU.
    Running,
    /// Waiting on IPC, a signal, or a timer.
    Blocked,
    /// Stopped by `SYS_THREAD_STOP`.
    Stopped,
    /// Finished; TCB will be freed.
    Exited,
}

// ── ThreadControlBlock ────────────────────────────────────────────────────────

/// Per-thread kernel state.
///
/// # Phase 8 scope
/// Only the scheduling and context fields are used. IPC fields and address-space
/// / cspace pointers are zeroed placeholders; their types will be concrete in
/// Phase 9/10.
///
/// # Safety invariant
/// `run_queue_next` and `ipc_wait_next` are raw intrusive pointers. They are
/// only valid when the TCB is on a run queue or IPC wait queue respectively.
/// Access is serialised by the owning CPU's `PerCpuScheduler` lock (Phase 9+).
///
/// # TODO Phase 9
/// - Replace `reply_cap_slot: u64`, `pending_send: u64`, `wakeup_value`,
///   `wakeup_token`, and `ipc_wait_next` with the proper IPC types from cap/.
///
/// # TODO Phase 10
/// - Replace `address_space: u64` and `cspace: u64` with typed pointers to
///   `AddressSpace` and `CSpace`.
/// - Bind `cpu_affinity` and `preferred_cpu` to the actual CPU being started.
#[repr(C)]
pub struct ThreadControlBlock
{
    // === Scheduling state ===
    /// Current lifecycle state.
    pub state: ThreadState,

    /// Scheduling priority (0 = idle, 1–30 = userspace, 31 = reserved).
    pub priority: u8,

    /// Remaining preemption timer ticks before this thread is descheduled.
    pub slice_remaining: u32,

    /// Hard CPU affinity (AFFINITY_ANY = 0xFFFF_FFFF means no hard affinity).
    /// TODO Phase 10: enforce during thread migration / load balancing.
    pub cpu_affinity: u32,

    /// Soft affinity: last CPU this thread ran on (hint for the load balancer).
    /// TODO Phase 10: update on each context switch.
    pub preferred_cpu: u32,

    /// Intrusive run-queue link — next TCB at the same priority.
    /// `None` when not on any run queue.
    pub run_queue_next: Option<*mut ThreadControlBlock>,

    // === IPC state (Phase 9 placeholder) ===
    // TODO Phase 9: replace with proper IPC types (ReplyCapability, PendingSendBuffer).
    /// Reply capability slot for a pending IPC call.
    pub reply_cap_slot: u64,

    /// Pending send message buffer (used while BlockedOnSend).
    pub pending_send: u64,

    /// Wakeup value — payload for signal/event wakeup.
    pub wakeup_value: u64,

    /// Token from a wait-set wakeup.
    pub wakeup_token: u64,

    /// Intrusive IPC wait-queue link.
    pub ipc_wait_next: Option<*mut ThreadControlBlock>,

    // === Context ===
    /// Architecture-specific saved kernel register state.
    pub saved_state: SavedState,

    /// Virtual address of the top of this thread's kernel stack.
    /// Stored in TSS RSP0 (x86-64) or sscratch (RISC-V) on every context switch.
    pub kernel_stack_top: u64,

    // === Address space / capability references (Phase 10 placeholder) ===
    // TODO Phase 10: replace with *mut AddressSpace and *mut CSpace.
    /// Address space this thread executes in (null for kernel threads).
    pub address_space: u64,

    /// CSpace bound to this thread.
    pub cspace: u64,

    // === Identity ===
    /// Unique thread identifier assigned at creation.
    pub thread_id: u32,
}

// SAFETY: TCB pointers are only accessed under the scheduler lock (Phase 9+).
// Phase 8 is single-threaded, so no concurrent access is possible.
unsafe impl Send for ThreadControlBlock {}
