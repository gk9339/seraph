// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/sched/mod.rs

//! Kernel scheduler — Phase 8/9/10: per-CPU state, idle threads, init launch,
//! and context switching.
//!
//! Phase 8 allocates a kernel stack and idle TCB for each CPU.
//! Phase 9 adds `enter()`, which dequeues the init thread, builds its initial
//! user-mode [`TrapFrame`], activates its address space, and calls
//! `return_to_user` to start init running.
//! Phase 10 adds `schedule()` for preemptive context switching and wires
//! timer preemption.
//!
//! # Deferred work
//! - Phase 11: SMP bringup, secondary CPU idle threads, load balancing.

#[cfg(not(test))]
extern crate alloc;

#[cfg(not(test))]
use alloc::boxed::Box;

pub mod run_queue;
pub mod thread;

use run_queue::PerCpuScheduler;
use thread::{IpcThreadState, ThreadControlBlock, ThreadState};

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

/// Priority assigned to the init process.
///
/// Higher than all idle threads (0) and general userspace (1–14); below the
/// reserved high-priority level (31).
pub const INIT_PRIORITY: u8 = 15;

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

/// Atomic counter for assigning unique thread IDs.
static NEXT_THREAD_ID: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

#[cfg(not(test))]
fn alloc_thread_id() -> u32
{
    NEXT_THREAD_ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed)
}

// ── Idle thread entry ─────────────────────────────────────────────────────────

/// Entry function for idle threads.
///
/// Runs at priority 0. If any runnable threads are available, yields to the
/// scheduler; otherwise halts the CPU until the next interrupt.
///
/// `_cpu_id` — logical CPU index (0-based).
fn idle_thread_entry(_cpu_id: u64) -> !
{
    loop
    {
        // Check non_empty before halting; if any threads are ready, schedule
        // them rather than spinning idle. SAFETY: SCHEDULERS[0] is owned by
        // the BSP; single-CPU Phase 10.
        #[cfg(not(test))]
        {
            let has_work = unsafe { SCHEDULERS[0].has_runnable() };
            if has_work
            {
                // SAFETY: single-CPU Phase 10; called from scheduler context.
                unsafe { schedule(); }
            }
        }
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
            ipc_state: IpcThreadState::None,
            ipc_msg: crate::ipc::message::Message::default(),
            reply_tcb: core::ptr::null_mut(),
            ipc_wait_next: None,
            is_user: false,
            saved_state: saved,
            kernel_stack_top: stack_top,
            trap_frame: core::ptr::null_mut(),
            address_space: core::ptr::null_mut(),
            cspace: core::ptr::null_mut(),
            ipc_buffer: 0,
            wakeup_value: 0,
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

// ── schedule ──────────────────────────────────────────────────────────────────

/// Select the next thread to run and switch to it.
///
/// Called from `sys_yield`, timer preemption, and the idle thread. On a
/// single CPU (Phase 10) this always uses `SCHEDULERS[0]`.
///
/// If the current thread is still `Running` it is re-queued at its priority
/// before selection. If the selected next thread is the same as the current
/// one, no switch occurs.
///
/// After updating architecture-specific kernel-stack pointers the scheduler
/// lock is released, then `arch::current::context::switch` performs the actual
/// register save/restore.
///
/// # Safety
/// Must be called from within a kernel context (interrupt handler or syscall
/// handler) with a valid kernel stack. Interrupts are disabled by the
/// scheduler lock; they are re-enabled as part of lock release.
#[cfg(not(test))]
pub unsafe fn schedule()
{
    use crate::arch::current::context::switch;
    use thread::ThreadState;

    let sched = unsafe { &mut SCHEDULERS[0] };

    // Acquire the scheduler lock via lock_raw so we hold no borrow reference
    // to `sched` during the critical section — allowing us to call mutable
    // methods on `sched` while the lock is logically held.
    // SAFETY: lock_raw must be paired with unlock_raw before function return
    // (or before the context switch that may change the stack).
    let saved_flags = unsafe { sched.lock.lock_raw() };

    let current = sched.current;

    // If the current thread is still actively running, put it back in the
    // run queue so it can be rescheduled.
    // SAFETY: current is a valid TCB set by enter() or a previous schedule().
    if !current.is_null()
    {
        unsafe {
            if (*current).state == ThreadState::Running
            {
                (*current).state = ThreadState::Ready;
                let prio = (*current).priority;
                sched.enqueue(current, prio);
            }
        }
    }

    let next = sched.dequeue_highest();

    // If the scheduler selected the same thread, nothing to do.
    if next == current
    {
        // Re-mark as running, release lock, and return.
        if !current.is_null()
        {
            unsafe { (*current).state = ThreadState::Running; }
        }
        unsafe { sched.lock.unlock_raw(saved_flags); }
        return;
    }

    // Activate next thread.
    // SAFETY: next is a valid TCB from the run queue or idle.
    unsafe {
        (*next).state = ThreadState::Running;
    }
    sched.set_current(next);

    // Update the kernel trap stack pointer so the next ring-3 → ring-0
    // transition (interrupt, exception, or syscall) lands on the correct
    // kernel stack for the incoming thread.
    // SAFETY: next is a valid TCB; called with interrupts disabled.
    unsafe { crate::arch::current::cpu::set_kernel_trap_stack((*next).kernel_stack_top); }

    // Switch address space if different (both non-null).
    // SAFETY: address_space pointers were set up by Phase 9 init or future
    // thread creation; null means kernel thread (shares kernel mappings).
    unsafe {
        let cur_as = if current.is_null() { core::ptr::null_mut() } else { (*current).address_space };
        let nxt_as = (*next).address_space;
        if !nxt_as.is_null() && nxt_as != cur_as
        {
            (*nxt_as).activate();
        }
    }

    // Capture saved-state pointers before releasing the lock.
    let current_state: *mut crate::arch::current::context::SavedState = if current.is_null()
    {
        core::ptr::null_mut()
    }
    else
    {
        // SAFETY: current is a valid TCB.
        unsafe { &mut (*current).saved_state as *mut _ }
    };
    // SAFETY: next is a valid TCB.
    let next_state = unsafe { &(*next).saved_state as *const _ };

    // Release the lock before calling switch. The context switch changes RSP
    // to the next thread's kernel stack; the unlock_raw call must complete on
    // the current stack before that happens.
    // SAFETY: saved_flags was returned by the matching lock_raw above.
    unsafe { sched.lock.unlock_raw(saved_flags); }

    if !current_state.is_null()
    {
        // SAFETY: both pointers are valid SavedState values on heap-allocated
        // TCBs; interrupts are now re-enabled.
        unsafe { switch(current_state, next_state); }
    }
    // If current_state is null (unreachable post-init), the idle TCB is
    // always current, so this path cannot be reached normally.
}

/// Decrement the current thread's time slice and call `schedule()` if expired.
///
/// Called from architecture-specific timer handlers on each timer tick.
///
/// # Safety
/// Must be called from within an interrupt handler with a valid kernel stack.
#[cfg(not(test))]
pub unsafe fn timer_tick()
{
    let sched = unsafe { &mut SCHEDULERS[0] };
    let current = sched.current;
    if current.is_null()
    {
        return;
    }
    // SAFETY: current is a valid TCB.
    let remaining = unsafe { (*current).slice_remaining };
    if remaining == 0
    {
        // Idle threads have slice_remaining = 0 and should not be preempted.
        return;
    }
    let new_remaining = remaining - 1;
    unsafe { (*current).slice_remaining = new_remaining; }
    if new_remaining == 0
    {
        // Reset slice for next run.
        unsafe { (*current).slice_remaining = TIME_SLICE_TICKS; }
        // SAFETY: called from interrupt handler.
        unsafe { schedule(); }
    }
}

// ── enter ─────────────────────────────────────────────────────────────────────

/// Start executing the highest-priority ready thread and never return.
///
/// Called once at the end of kernel boot after the init TCB has been enqueued.
/// Dequeues the init thread, activates its address space, sets TSS RSP0 /
/// SYSCALL_KERNEL_RSP, builds an initial user-mode [`TrapFrame`] on its kernel
/// stack, and calls `return_to_user`.
///
/// # Panics
/// Calls `crate::fatal` if the run queue is empty (init TCB not enqueued).
///
/// # Safety
/// Must be called exactly once, from the single boot thread, after:
/// - Phase 3 (page tables active)
/// - Phase 4 (heap active)
/// - Phase 8 scheduler init
/// - Phase 9 init TCB enqueued on BSP run queue
#[cfg(not(test))]
pub fn enter() -> !
{
    use crate::arch::current::trap_frame::TrapFrame;
    use crate::mm::address_space::INIT_STACK_TOP;

    // Dequeue the highest-priority ready thread (init, at INIT_PRIORITY=15).
    // SAFETY: single-threaded boot; SCHEDULERS[0] is exclusively owned here.
    let init_tcb = unsafe {
        let sched = &mut SCHEDULERS[0];
        let tcb = sched.dequeue_highest();
        if tcb.is_null()
        {
            crate::fatal("sched::enter: run queue empty — init TCB not enqueued");
        }
        // Mark as the currently running thread so syscall handlers that call
        // current_tcb() find the correct TCB while init is executing.
        sched.set_current(tcb);
        (*tcb).state = thread::ThreadState::Running;
        &mut *tcb
    };

    let kernel_stack_top = init_tcb.kernel_stack_top;

    // Retrieve the user entry point stored in saved_state at TCB creation.
    let entry_point = init_tcb.saved_state.entry_point();

    // Set the kernel trap stack pointer before entering user mode so the first
    // ring-3 → ring-0 transition lands on the correct kernel stack.
    // On x86-64: writes TSS RSP0 + SYSCALL_KERNEL_RSP.
    // On RISC-V: writes sscratch (read by trap_entry to switch stacks).
    // SAFETY: single-boot-thread; kernel_stack_top is valid.
    unsafe { crate::arch::current::cpu::set_kernel_trap_stack(kernel_stack_top); }

    // Build the initial user-mode TrapFrame on the init thread's kernel stack.
    // The frame sits just below kernel_stack_top.
    let tf_size = core::mem::size_of::<TrapFrame>() as u64;
    let tf_ptr = (kernel_stack_top - tf_size) as *mut TrapFrame;

    // Zero the frame then populate the user-mode entry fields via TrapFrame
    // methods (arch-specific field names are hidden inside trap_frame.rs).
    // SAFETY: kernel_stack_top - tf_size is within the allocated kernel stack.
    unsafe {
        core::ptr::write_bytes(tf_ptr as *mut u8, 0, tf_size as usize);
        (*tf_ptr).init_user(entry_point, INIT_STACK_TOP);
    }

    // Record the trap_frame pointer in the TCB so future trap handlers can
    // find the correct frame when the thread is running.
    init_tcb.trap_frame = tf_ptr;

    // Read init's page table root before entering the switch function.
    // SAFETY: init_tcb.address_space was set up in main.rs Phase 9 init.
    let root_phys = unsafe { (*init_tcb.address_space).root_phys };

    crate::kprintln!("  sched::enter: handing control to init");

    // Activate init's address space and enter user mode.
    // `first_entry_to_user` handles the arch-specific sequence:
    //   x86-64: atomically switches CR3 and executes iretq from init's kernel stack.
    //   RISC-V: writes satp (sfence serializes), then executes sret.
    // SAFETY: root_phys is init's valid page-table root; tf_ptr is on init's kernel stack.
    unsafe { crate::arch::current::context::first_entry_to_user(root_phys, tf_ptr); }
}
