// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/sched/thread.rs

//! Thread Control Block (TCB) definition.
//!
//! Each kernel thread has exactly one TCB. TCBs are heap-allocated via `Box`.
//!
//! Key fields:
//! - `address_space`: typed pointer to the user address space (null for kernel threads).
//! - `cspace`: typed pointer to the capability space.
//! - `ipc_state`: IPC blocking state.
//! - `ipc_msg`: inline message buffer for IPC transfer.
//! - `reply_tcb`: pointer to the thread to wake on IPC reply.
//! - `trap_frame`: pointer to the user register snapshot on the kernel stack.
//! - `is_user`: true for user-mode threads.
//! - `ipc_buffer`: virtual address of the per-thread IPC buffer page (0 = none).
//! - `wakeup_value`: value delivered by a signal sender to an unblocked waiter.

use crate::arch::current::context::SavedState;

// ── IpcThreadState ────────────────────────────────────────────────────────────

/// IPC blocking reason for a thread in the `Blocked` state.
///
/// Threads not involved in IPC have `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcThreadState
{
    /// Not blocked on IPC.
    None,
    /// Blocked waiting for a receiver to call `recv` on an endpoint.
    BlockedOnSend,
    /// Blocked waiting for a caller to `call` an endpoint.
    BlockedOnRecv,
    /// Blocked waiting for a `reply` after a `call`.
    BlockedOnReply,
    /// Blocked waiting for a signal bitmask to become non-zero.
    BlockedOnSignal,
    /// Blocked waiting for an event queue to receive an entry.
    BlockedOnEventQueue,
    /// Blocked waiting for any member of a wait set to become ready.
    BlockedOnWaitSet,
}

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
/// # Safety invariant
/// `run_queue_next` and `ipc_wait_next` are raw intrusive pointers. They are
/// only valid when the TCB is on a run queue or IPC wait queue respectively.
/// Access is serialised by the owning CPU's `PerCpuScheduler` lock.
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

    /// Hard CPU affinity (`AFFINITY_ANY` = `0xFFFF_FFFF` means no hard affinity).
    /// TODO: enforce during thread migration / load balancing.
    pub cpu_affinity: u32,

    /// Soft affinity: last CPU this thread ran on (hint for the load balancer).
    /// Updated by `schedule()` on each context switch.
    pub preferred_cpu: u32,

    /// Intrusive run-queue link — next TCB at the same priority.
    /// `None` when not on any run queue.
    pub run_queue_next: Option<*mut ThreadControlBlock>,

    // === IPC state ===
    /// Current IPC blocking reason (None when not blocked on IPC).
    pub ipc_state: IpcThreadState,

    /// Inline message buffer for in-flight IPC data.
    pub ipc_msg: crate::ipc::message::Message,

    /// Thread waiting for our reply (set when we received a call; cleared on reply).
    pub reply_tcb: *mut ThreadControlBlock,

    /// Intrusive IPC wait-queue link.
    pub ipc_wait_next: Option<*mut ThreadControlBlock>,

    // === Context ===
    /// Whether this thread executes in user mode (ring 3 / U-mode).
    pub is_user: bool,

    /// Architecture-specific saved kernel register state.
    pub saved_state: SavedState,

    /// Virtual address of the top of this thread's kernel stack.
    /// Stored in TSS RSP0 (x86-64) or sscratch (RISC-V) on every context switch.
    pub kernel_stack_top: u64,

    /// Pointer to the `TrapFrame` on the kernel stack (null for kernel threads).
    ///
    /// Populated by `syscall_entry` / trap handler on each kernel entry.
    /// Points into the kernel stack below `kernel_stack_top`.
    pub trap_frame: *mut crate::arch::current::trap_frame::TrapFrame,

    // === Address space / capability references ===
    /// Address space this thread executes in (null for kernel threads).
    pub address_space: *mut crate::mm::address_space::AddressSpace,

    /// `CSpace` bound to this thread.
    pub cspace: *mut crate::cap::cspace::CSpace,

    // === IPC buffer ===
    /// Virtual address of the per-thread IPC buffer page (0 = not registered).
    ///
    /// Registered by `SYS_IPC_BUFFER_SET`. IPC data words are read from / written
    /// to this page when `data_count > 0`.
    pub ipc_buffer: u64,

    /// Wakeup value delivered to this thread when unblocked from a signal wait.
    ///
    /// Set by `signal_send` when it wakes a blocked waiter: stores the bits that
    /// were acquired on the waiter's behalf. Read by `sys_signal_wait` on resume.
    pub wakeup_value: u64,

    // === I/O port permissions (x86_64 only) ===
    /// Per-thread I/O Permission Bitmap (8 KiB, heap-allocated on first
    /// `SYS_IOPORT_BIND`). Null if this thread has no port bindings.
    ///
    /// On context switch, if non-null, this bitmap is copied into the TSS
    /// IOPB region so `in`/`out` instructions work for this thread.
    ///
    // TODO: When an IoPortRange cap (or ancestor) is revoked,
    // the relevant bits must be re-denied in this bitmap and reloaded into
    // the TSS if this thread is currently running. Requires tracking which
    // threads hold which IoPortRange bindings. Pick up alongside general
    // cap revocation side-effect cleanup.
    pub iopb: *mut [u8; crate::arch::current::IOPB_SIZE],

    // === IPC block cancellation ===
    /// Pointer to the kernel IPC object this thread is currently blocked on
    /// (null when not blocked). Cast to the concrete type using `ipc_state`:
    /// - `BlockedOnSend`/`BlockedOnRecv` → `*mut EndpointState`
    /// - `BlockedOnSignal` → `*mut SignalState`
    /// - `BlockedOnEventQueue` → `*mut EventQueueState`
    /// - `BlockedOnWaitSet` → `*mut WaitSetState`
    ///
    /// Set when entering any IPC-blocked state; cleared on wakeup.
    /// Used by `SYS_THREAD_STOP` to unlink the thread from the blocking queue.
    pub blocked_on_object: *mut u8,

    // === Identity ===
    /// Unique thread identifier assigned at creation.
    pub thread_id: u32,

    // === Context switch synchronisation ===
    /// Cleared before `release_lock_only()` in `schedule()`, set after
    /// `switch()` has finished saving this thread's registers. A remote
    /// CPU that dequeues this thread spins on this flag (Acquire) before
    /// loading its `SavedState`, ensuring the save is globally visible
    /// on RISC-V RVWMO.
    pub context_saved: core::sync::atomic::AtomicU32,

    // === Use-after-free detection ===
    /// Magic cookie for use-after-free detection. Must be `TCB_MAGIC` when valid.
    pub magic: u64,
}

/// Expected value of `ThreadControlBlock::magic` for a live TCB.
pub const TCB_MAGIC: u64 = 0xDEAD_BEEF_CAFE_F00D;

// SAFETY: TCB pointers are only accessed under the scheduler lock.
unsafe impl Send for ThreadControlBlock {}
