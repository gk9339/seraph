// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/sched/run_queue.rs

//! Per-CPU run queue and scheduler state.
//!
//! [`PerCpuScheduler`] owns 32 priority queues (one per level), a bitmask of
//! non-empty queues for O(1) highest-priority selection, and pointers to the
//! currently running and idle TCBs.
//!
//! # Phase 9 scope
//! The `lock` field is a real `Spinlock<()>` (implemented in Phase 9). It
//! disables interrupts while held, preventing timer-driven deadlock.
//! Acquire before any `enqueue`, `dequeue_highest`, or `set_current` call.
//!
//! # TODO Phase 9
//! - Invoke `arch::current::gdt::set_rsp0` (x86-64) or update `sscratch` (RISC-V)
//!   with `next_tcb.kernel_stack_top` inside `context_switch`.
//!
//! # TODO Phase 10 (SMP)
//! - Implement `load_balance` across CPUs.
//! - Add `preemption_pending: bool` flag per CPU.

use super::thread::ThreadControlBlock;
use super::NUM_PRIORITY_LEVELS;

// ── RunQueue ──────────────────────────────────────────────────────────────────

/// Intrusive FIFO queue of ready TCBs at a single priority level.
struct RunQueue
{
    head: Option<*mut ThreadControlBlock>,
    tail: Option<*mut ThreadControlBlock>,
}

impl RunQueue
{
    const fn new() -> Self
    {
        Self {
            head: None,
            tail: None,
        }
    }

    /// Append `tcb` to the tail of the queue (FIFO scheduling within a priority).
    fn enqueue(&mut self, tcb: *mut ThreadControlBlock)
    {
        // SAFETY: tcb is a valid heap-allocated TCB pointer.
        unsafe { (*tcb).run_queue_next = None };

        match self.tail
        {
            None =>
            {
                self.head = Some(tcb);
                self.tail = Some(tcb);
            }
            Some(tail) =>
            {
                // SAFETY: tail is a valid TCB.
                unsafe { (*tail).run_queue_next = Some(tcb) };
                self.tail = Some(tcb);
            }
        }
    }

    /// Remove and return the head TCB, or `None` if empty.
    fn dequeue(&mut self) -> Option<*mut ThreadControlBlock>
    {
        let head = self.head?;
        // SAFETY: head is a valid TCB.
        self.head = unsafe { (*head).run_queue_next };
        if self.head.is_none()
        {
            self.tail = None;
        }
        // SAFETY: head is a valid TCB.
        unsafe { (*head).run_queue_next = None };
        Some(head)
    }

    fn is_empty(&self) -> bool
    {
        self.head.is_none()
    }
}

// ── PerCpuScheduler ───────────────────────────────────────────────────────────

/// Per-CPU scheduler state: priority run queues, current thread, and idle thread.
pub struct PerCpuScheduler
{
    /// One FIFO run queue per priority level (0 = lowest/idle, 31 = highest).
    queues: [RunQueue; NUM_PRIORITY_LEVELS],

    /// Bitmask: bit N is set iff `queues[N]` is non-empty.
    /// Enables O(1) selection of the highest non-empty priority queue.
    non_empty: u32,

    /// Currently executing TCB on this CPU (non-null after `init`).
    pub current: *mut ThreadControlBlock,

    /// Idle TCB for this CPU (non-null after `init`).
    pub idle: *mut ThreadControlBlock,

    /// Lock protecting this struct.
    ///
    /// Acquire before any enqueue/dequeue/set_current operation.
    /// The lock disables interrupts while held, preventing timer-driven deadlock.
    pub lock: crate::sync::Spinlock<()>,
}

// SAFETY: scheduler is protected by `lock` (Phase 9+) and only accessed
// from the owning CPU in Phase 8 (single-threaded boot).
unsafe impl Send for PerCpuScheduler {}
unsafe impl Sync for PerCpuScheduler {}

// RunQueue does not implement Copy/Clone, so we cannot derive Default or use
// array repeat syntax. Provide a manual const constructor instead.
impl PerCpuScheduler
{
    /// Construct an uninitialized (zeroed) scheduler state.
    ///
    /// `init()` in `sched/mod.rs` populates `current` and `idle` before use.
    pub const fn new() -> Self
    {
        // Manually expand the 32-element array because `RunQueue` is not Copy.
        // If NUM_PRIORITY_LEVELS changes, update this list accordingly.
        // TODO: switch to `[const { RunQueue::new() }; N]` once that syntax
        // stabilises in the kernel's MSRV.
        const Q: RunQueue = RunQueue::new();
        Self {
            queues: [
                Q, Q, Q, Q, Q, Q, Q, Q, Q, Q, Q, Q, Q, Q, Q, Q, Q, Q, Q, Q, Q, Q, Q, Q, Q, Q, Q, Q,
                Q, Q, Q, Q,
            ],
            non_empty: 0,
            current: core::ptr::null_mut(),
            idle: core::ptr::null_mut(),
            lock: crate::sync::Spinlock::new(()),
        }
    }

    /// Enqueue `tcb` at the given `priority` level.
    ///
    /// Sets bit `priority` in `non_empty`.
    pub fn enqueue(&mut self, tcb: *mut ThreadControlBlock, priority: u8)
    {
        let p = priority as usize;
        debug_assert!(p < NUM_PRIORITY_LEVELS, "priority out of range");
        self.queues[p].enqueue(tcb);
        self.non_empty |= 1 << p;
    }

    /// Dequeue the highest-priority ready TCB, or return `idle` if all queues
    /// are empty.
    ///
    /// Clears the `non_empty` bit if the queue at that priority becomes empty.
    pub fn dequeue_highest(&mut self) -> *mut ThreadControlBlock
    {
        if self.non_empty == 0
        {
            return self.idle;
        }
        // Highest set bit gives the highest non-empty priority level.
        let priority = 31 - self.non_empty.leading_zeros() as usize;
        let tcb = self.queues[priority]
            .dequeue()
            .expect("non_empty bit set but queue is empty");
        if self.queues[priority].is_empty()
        {
            self.non_empty &= !(1 << priority);
        }
        tcb
    }

    /// Record `tcb` as the idle thread for this CPU.
    pub fn set_idle(&mut self, tcb: *mut ThreadControlBlock)
    {
        self.idle = tcb;
    }

    /// Record `tcb` as the currently running thread on this CPU.
    pub fn set_current(&mut self, tcb: *mut ThreadControlBlock)
    {
        self.current = tcb;
    }

    /// Return `true` if any thread is ready to run (non-empty run queues).
    pub fn has_runnable(&self) -> bool
    {
        self.non_empty != 0
    }
}

// RunQueue is not Copy, so derive Copy on the containing const is not possible.
// Provide the impl manually for the const array construction above.
impl Copy for RunQueue {}
impl Clone for RunQueue
{
    fn clone(&self) -> Self
    {
        *self
    }
}
