// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/sched/mod.rs

//! Kernel scheduler — Phase 8: per-CPU state and idle thread initialisation.
//!
//! Phase 8 allocates a kernel stack and idle TCB for each CPU and wires them
//! into the per-CPU [`PerCpuScheduler`]. No scheduling decisions are made yet;
//! the BSP continues into Phase 9 immediately after `init` returns.
//!
//! # Deferred work
//! - Phase 9: `context_switch`, preemption timer handler, `schedule()` call.
//! - Phase 10: SMP bringup, secondary CPU idle threads, load balancing.
//! - Post Phase 10: slab cache optimisation for TCB allocation.

#[cfg(not(test))]
extern crate alloc;

#[cfg(not(test))]
use alloc::boxed::Box;

pub mod run_queue;
pub mod thread;

use run_queue::PerCpuScheduler;
use thread::{ThreadControlBlock, ThreadState};

use crate::arch::current::context::new_state;
use crate::arch::current::cpu::halt_until_interrupt;
use crate::mm::paging::phys_to_virt;
use crate::mm::{BuddyAllocator, PAGE_SIZE};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Number of priority levels. Matches the `non_empty: u32` bitmask width.
pub const NUM_PRIORITY_LEVELS: usize = 32;

/// Priority assigned to idle threads.
pub const IDLE_PRIORITY: u8 = 0;

/// Default time-slice length in preemption-timer ticks.
/// TODO Phase 9: decrement in the timer interrupt handler.
pub const TIME_SLICE_TICKS: u32 = 10;

/// Number of 4 KiB pages in each idle thread's kernel stack (16 KiB total).
pub const KERNEL_STACK_PAGES: usize = 4;

/// Maximum number of CPUs. Matches the `u64` TLB-shootdown cpu-mask width.
/// TODO Phase 10: enforce during SMP bringup if cpu_count exceeds this.
pub const MAX_CPUS: usize = 64;

/// Hard affinity sentinel: no hard CPU affinity.
pub const AFFINITY_ANY: u32 = 0xFFFF_FFFF;

// ── Global per-CPU scheduler array ────────────────────────────────────────────

/// One `PerCpuScheduler` per potential CPU.
///
/// Indexed by logical CPU ID (0-based). Only entries 0..cpu_count are
/// initialised by `init`.
///
/// # Safety
/// Accessed exclusively from the owning CPU after SMP bringup (Phase 10).
/// During Phase 8 there is only one CPU, so no concurrent access is possible.
// SAFETY: single-threaded Phase 8 boot; SMP lock enforced from Phase 10.
#[cfg(not(test))]
static mut SCHEDULERS: [PerCpuScheduler; MAX_CPUS] = {
    // Manual const-init: PerCpuScheduler is not Copy, so we cannot use
    // array repeat syntax directly. A const block evaluating to a fixed
    // 64-element literal is the correct approach until `[expr; N]` with
    // non-Copy types is stabilised.
    const S: PerCpuScheduler = PerCpuScheduler::new();
    [
        S, S, S, S, S, S, S, S, // 8
        S, S, S, S, S, S, S, S, // 16
        S, S, S, S, S, S, S, S, // 24
        S, S, S, S, S, S, S, S, // 32
        S, S, S, S, S, S, S, S, // 40
        S, S, S, S, S, S, S, S, // 48
        S, S, S, S, S, S, S, S, // 56
        S, S, S, S, S, S, S, S, // 64
    ]
};

// ── Thread ID counter ─────────────────────────────────────────────────────────

// Simple counter for assigning unique thread IDs.
// TODO Phase 9: replace with an atomic counter once multiple threads are created.
static mut NEXT_THREAD_ID: u32 = 0;

#[cfg(not(test))]
fn alloc_thread_id() -> u32
{
    // SAFETY: single-threaded Phase 8 boot; no concurrent access.
    unsafe {
        let id = NEXT_THREAD_ID;
        NEXT_THREAD_ID = id.wrapping_add(1);
        id
    }
}

// ── Idle thread entry ─────────────────────────────────────────────────────────

/// Entry function for idle threads.
///
/// Runs at priority 0. Halts the CPU until the next interrupt, then loops.
/// The `schedule()` call that dispatches real work is added in Phase 9.
///
/// `_cpu_id` — logical CPU index (0-based).
///
/// # TODO Phase 9
/// Before halting, check whether any thread became runnable (to avoid a race
/// between the runnable check and the halt instruction):
/// ```rust
/// if has_runnable_threads(cpu_id) { schedule(); }
/// ```
fn idle_thread_entry(_cpu_id: u64) -> !
{
    loop
    {
        halt_until_interrupt();
    }
}

// ── init ──────────────────────────────────────────────────────────────────────

/// Initialise per-CPU scheduler state and idle threads for `cpu_count` CPUs.
///
/// For each CPU:
/// 1. Allocates `KERNEL_STACK_PAGES` physical frames from the buddy allocator.
/// 2. Converts the physical base to a virtual address via the direct map.
/// 3. Creates an idle [`ThreadControlBlock`] with initial context pointing at
///    [`idle_thread_entry`].
/// 4. Registers the TCB as both `idle` and `current` in the CPU's scheduler.
///
/// Returns `cpu_count` (for use in the Phase 8 startup log message).
///
/// # Panics
/// Halts with `fatal()` if the buddy allocator cannot satisfy a stack
/// allocation request.
///
/// # Safety
/// Must be called exactly once, from the single boot thread, after Phase 3
/// (page tables active) and Phase 4 (heap active).
#[cfg(not(test))]
pub fn init(cpu_count: u32, allocator: &mut BuddyAllocator) -> u32
{
    // Order for KERNEL_STACK_PAGES (4 pages = order 2).
    // 2^order pages >= KERNEL_STACK_PAGES.
    let stack_order = {
        let mut o = 0;
        while (1usize << o) < KERNEL_STACK_PAGES
        {
            o += 1;
        }
        o
    };

    for cpu in 0..cpu_count as usize
    {
        // 1. Allocate stack frames.
        let stack_phys = allocator
            .alloc(stack_order)
            .unwrap_or_else(|| crate::fatal("sched::init: out of memory for idle stack"));

        // 2. Convert physical address to virtual (direct map).
        let stack_virt = phys_to_virt(stack_phys);

        // Stack grows downward; top = base + size.
        let stack_top = stack_virt + (KERNEL_STACK_PAGES * PAGE_SIZE) as u64;

        // 3. Build idle TCB.
        let saved = new_state(
            idle_thread_entry as *const () as u64,
            stack_top,
            cpu as u64,
            false,
        );

        let tcb = Box::into_raw(Box::new(ThreadControlBlock {
            state: ThreadState::Running,
            priority: IDLE_PRIORITY,
            slice_remaining: 0, // idle threads are never preempted by the timer
            cpu_affinity: cpu as u32,
            preferred_cpu: cpu as u32,
            run_queue_next: None,
            reply_cap_slot: 0,
            pending_send: 0,
            wakeup_value: 0,
            wakeup_token: 0,
            ipc_wait_next: None,
            saved_state: saved,
            kernel_stack_top: stack_top,
            address_space: 0,
            cspace: 0,
            thread_id: alloc_thread_id(),
        }));

        // 4. Register in per-CPU scheduler.
        // SAFETY: single-threaded boot; SCHEDULERS[cpu] is not accessed elsewhere.
        unsafe {
            SCHEDULERS[cpu].set_idle(tcb);
            SCHEDULERS[cpu].set_current(tcb);
        }
    }

    cpu_count
}

// ── Test stub ─────────────────────────────────────────────────────────────────

/// No-op stub used when the kernel crate is compiled for host tests.
///
/// `kernel_entry` (in main.rs) is compiled in test mode even though it is
/// never called; this stub satisfies the call site without requiring access to
/// arch-specific or heap types that are unavailable on the host.
#[cfg(test)]
#[allow(unused_variables)]
pub fn init(_cpu_count: u32, _allocator: &mut crate::mm::BuddyAllocator) -> u32
{
    0
}

// ── Public accessor ───────────────────────────────────────────────────────────

/// Return a reference to the scheduler for CPU `id`.
///
/// # Safety
/// The caller must ensure `id < MAX_CPUS` and that `init` has been called for
/// this CPU. No concurrent mutable access may occur without holding the
/// scheduler lock (Phase 9+).
#[cfg(not(test))]
#[allow(dead_code)]
pub unsafe fn scheduler_for(id: usize) -> &'static mut PerCpuScheduler
{
    debug_assert!(id < MAX_CPUS);
    // SAFETY: caller's responsibility.
    unsafe { &mut SCHEDULERS[id] }
}
