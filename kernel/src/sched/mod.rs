// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/sched/mod.rs

//! Kernel scheduler — per-CPU state, idle threads, init launch, and context switching.
//!
//! Phase 8 allocates a kernel stack and idle TCB for each CPU.
//! Phase 9 adds `enter()`, which dequeues the init thread, builds its initial
//! user-mode [`TrapFrame`], activates its address space, and calls
//! `return_to_user` to start init running. `schedule()` provides preemptive
//! context switching; timer preemption decrements `slice_remaining` per tick.
//!
//! # Deferred work
//! - WSMP: SMP bringup, secondary CPU idle threads, load balancing.

// cast_possible_truncation: usize→u32 CPU index and u64→usize address bounded by MAX_CPUS.
#![allow(clippy::cast_possible_truncation)]

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
/// TODO WSMP: enforce during SMP bringup if `cpu_count` exceeds this.
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
/// Indexed by logical CPU ID (0-based). Only entries `0..cpu_count` are
/// initialised by `init`.
///
/// # Safety
/// Accessed exclusively from the owning CPU after SMP bringup (WSMP).
/// During Phase 8 there is only one CPU, so no concurrent access is possible.
// SAFETY: single-threaded Phase 8 boot; real per-CPU locks required for WSMP.
#[cfg(not(test))]
static mut SCHEDULERS: [PerCpuScheduler; MAX_CPUS] = {
    // Manual const-init: PerCpuScheduler is not Copy, so we cannot use
    // array repeat syntax directly. A const block evaluating to a fixed
    // 64-element literal is the correct approach until `[expr; N]` with
    // non-Copy types is stabilised.
    // declare_interior_mutable_const: S is only used to copy-initialise the static
    // array below; it is never used as a shared mutable reference.
    #[allow(clippy::declare_interior_mutable_const)]
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

/// Number of CPUs initialised by `sched::init`.
///
/// Written once during boot by `init`, then read by `SYS_SYSTEM_INFO(CpuCount)`.
pub static CPU_COUNT: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// Allocate a unique thread ID.
///
/// Called during idle thread creation, init TCB creation, and
/// `SYS_CAP_CREATE_THREAD`. IDs are monotonically increasing and never reused.
#[cfg(not(test))]
pub fn alloc_thread_id() -> u32
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
    // Enable supervisor interrupts so wfi can be woken by the timer.
    // new_state() sets sstatus.SIE=0 to prevent a race in switch() where
    // an interrupt during the register-restore window corrupts sscratch
    // (see context.rs comment). Idle must enable SIE explicitly here;
    // sscratch=0 at this point (idle is !is_user) so any S-mode interrupt
    // correctly takes the S-mode trap path.
    #[cfg(not(test))]
    // SAFETY: idle thread runs in supervisor mode with sscratch=0; enabling
    // interrupts allows wfi wakeup without corrupting user-mode state.
    unsafe {
        crate::arch::current::interrupts::enable();
    }

    loop
    {
        // Check this CPU's run queue before halting. Use current_cpu() so APs
        // consult their own scheduler rather than the BSP's (SCHEDULERS[0]).
        // schedule() still uses SCHEDULERS[0] (Phase D fix); this check is
        // safe as long as APs have empty run queues (true until Phase D).
        #[cfg(not(test))]
        {
            let cpu = crate::arch::current::cpu::current_cpu() as usize;
            // SAFETY: single-CPU system (Phase 8); CPU 0 scheduler is always valid.
            // WSMP Phase D will fix this for per-CPU AP schedulers.
            let has_work = unsafe { SCHEDULERS[cpu].has_runnable() };
            if has_work
            {
                // SAFETY: called from scheduler context on a valid kernel stack.
                unsafe {
                    schedule();
                }
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
// needless_range_loop: SCHEDULERS is static mut; iter_mut() requires unsafe coercion
// that is less clear than explicit indexing here.
#[allow(clippy::needless_range_loop)]
#[cfg(not(test))]
pub fn init(cpu_count: u32, allocator: &mut BuddyAllocator) -> u32
{
    CPU_COUNT.store(cpu_count, core::sync::atomic::Ordering::Relaxed);

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
            iopb: core::ptr::null_mut(),
            blocked_on_object: core::ptr::null_mut(),
            thread_id: alloc_thread_id(),
        }));

        // 4. Register in per-CPU scheduler.
        // SAFETY: single-threaded boot; SCHEDULERS[cpu] is exclusively owned during init.
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

// ── AP entry and helpers ──────────────────────────────────────────────────────

/// Enter the idle loop for an AP (Application Processor).
///
/// Called from `kernel_entry_ap` after per-CPU hardware initialisation is
/// complete. The AP has an empty run queue at this point; it idles until
/// the scheduler enqueues work (Phase D / F will add cross-CPU wakeup).
///
/// This function never returns.
///
/// # Safety
/// Must be called exactly once per AP, from the AP being initialised, after
/// per-CPU GDT, IDT, LAPIC, and SYSCALL have been set up.
#[cfg(not(test))]
pub fn ap_enter(cpu_id: u32) -> !
{
    // Idle loop: wait for interrupt. The idle TCB was created by sched::init();
    // SCHEDULERS[cpu_id].current already points to it. Interrupts are enabled
    // by idle_thread_entry which is the natural entry point of the idle thread.
    // We call it directly since we are "on" the idle thread's kernel stack.
    idle_thread_entry(u64::from(cpu_id))
}

/// Return the kernel stack top for the idle thread on CPU `cpu_id`.
///
/// Used by the BSP AP startup sequence to retrieve the idle stack address
/// for loading into the trampoline parameters and TSS RSP0.
///
/// # Safety
/// `cpu_id` must be < [`MAX_CPUS`] and `sched::init` must have been called
/// for this CPU.
#[cfg(not(test))]
pub unsafe fn idle_stack_top_for(cpu_id: usize) -> u64
{
    // SAFETY: caller guarantees cpu_id is valid and sched::init was called;
    // idle TCB pointer is non-null; kernel_stack_top field is always valid.
    unsafe { (*SCHEDULERS[cpu_id].idle).kernel_stack_top }
}

// ── Public accessor ───────────────────────────────────────────────────────────

/// Return a reference to the scheduler for CPU `id`.
///
/// # Safety
/// The caller must ensure `id < MAX_CPUS` and that `init` has been called for
/// this CPU. No concurrent mutable access may occur without holding the
/// scheduler lock (Phase 9+).
#[cfg(not(test))]
#[allow(dead_code)] // Multi-CPU accessor; called once SMP bringup is implemented.
pub unsafe fn scheduler_for(id: usize) -> &'static mut PerCpuScheduler
{
    debug_assert!(id < MAX_CPUS);
    // SAFETY: caller guarantees id < MAX_CPUS and exclusive access to this CPU's scheduler.
    unsafe { &mut SCHEDULERS[id] }
}

// ── schedule ──────────────────────────────────────────────────────────────────

/// Select the next thread to run and switch to it.
///
/// Called from `sys_yield`, timer preemption, and the idle thread. On a
/// single CPU (until WSMP) this always uses `SCHEDULERS[0]`.
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

    // SAFETY: single-CPU system; CPU 0 scheduler is always valid.
    let sched =
        unsafe { &mut SCHEDULERS[crate::arch::current::cpu::current_cpu() as usize] };

    // Acquire the scheduler lock via lock_raw so we hold no borrow reference
    // to `sched` during the critical section — allowing us to call mutable
    // methods on `sched` while the lock is logically held.
    // SAFETY: lock_raw must be paired with unlock_raw before function return
    // (or before the context switch that may change the stack).
    let saved_flags = unsafe { sched.lock.lock_raw() };

    let current = sched.current;

    // If the current thread is still actively running, put it back in the
    // run queue so it can be rescheduled.
    if !current.is_null()
    {
        // SAFETY: current is a valid TCB set by enter() or a previous schedule();
        // state, priority fields are always valid.
        unsafe {
            if (*current).state == ThreadState::Running
            {
                (*current).state = ThreadState::Ready;
                let prio = (*current).priority;
                sched.enqueue(current, prio);
            }
        }
    }

    // Skip any Stopped or Exited threads that may still be in the run queue.
    // A thread can be Stopped while Ready (before it was dequeued); the skip
    // loop drains those stale entries without deadlock because dequeue_highest
    // returns idle when all queues are empty.
    let mut next = sched.dequeue_highest();
    while !core::ptr::eq(next, sched.idle)
        && matches!(
            // SAFETY: next is a valid TCB from the run queue; state field is always valid.
            unsafe { (*next).state },
            ThreadState::Stopped | ThreadState::Exited
        )
    {
        next = sched.dequeue_highest();
    }

    // If the scheduler selected the same thread, nothing to do.
    if next == current
    {
        // Re-mark as running, release lock, and return.
        if !current.is_null()
        {
            // SAFETY: current is a valid TCB; state field is always valid.
            unsafe {
                (*current).state = ThreadState::Running;
            }
        }
        // SAFETY: saved_flags was returned by the matching lock_raw above.
        unsafe {
            sched.lock.unlock_raw(saved_flags);
        }
        return;
    }

    // Activate next thread.
    // SAFETY: next is a valid TCB from the run queue or idle; state field is always valid.
    unsafe {
        (*next).state = ThreadState::Running;
    }
    sched.set_current(next);

    // Update the kernel trap stack pointer so the next ring-3 → ring-0
    // transition (interrupt, exception, or syscall) lands on the correct
    // kernel stack for the incoming thread.
    //
    // On x86-64: writes TSS RSP0 + SYSCALL_KERNEL_RSP.
    // On RISC-V: writes PerCpuData::kernel_rsp (offset 8 from tp); sscratch
    //   is set to &PER_CPU by return_to_user just before sret, so trap_entry
    //   can detect U-mode (sscratch != 0) and recover tp.
    //   For kernel threads (idle): pass 0 to keep PerCpuData::kernel_rsp=0;
    //   sscratch is already 0 (S-mode invariant) so trap_entry takes the
    //   S-mode path correctly.
    //
    // SAFETY: next is a valid TCB; is_user and kernel_stack_top fields are always valid.
    let trap_stack = if unsafe { (*next).is_user }
    {
        // SAFETY: next is a valid TCB; kernel_stack_top field is always valid.
        unsafe { (*next).kernel_stack_top }
    }
    else
    {
        0
    };
    // SAFETY: trap_stack is valid (0 for kernel threads, kernel_stack_top for user threads);
    // interrupts are disabled by the scheduler lock.
    unsafe {
        crate::arch::current::cpu::set_kernel_trap_stack(trap_stack);
    }

    // Switch address space if different (both non-null).
    // SAFETY: current and next are valid TCBs; address_space pointers were set up
    // by Phase 9 init or thread creation; null means kernel thread (shares kernel mappings).
    unsafe {
        let cur_as = if current.is_null()
        {
            core::ptr::null_mut()
        }
        else
        {
            (*current).address_space
        };
        let nxt_as = (*next).address_space;
        if !nxt_as.is_null() && nxt_as != cur_as
        {
            (*nxt_as).activate();
        }
    }

    // Load the per-thread IOPB into the TSS (x86_64 only).
    // If the thread has no port bindings, fill the TSS IOPB with 0xFF (deny all).
    #[cfg(all(not(test), target_arch = "x86_64"))]
    // SAFETY: next is a valid TCB; iopb pointer is null or a valid heap-allocated [u8; IOPB_SIZE].
    unsafe {
        let iopb_ptr = (*next).iopb;
        if iopb_ptr.is_null()
        {
            crate::arch::current::gdt::load_iopb(None);
        }
        else
        {
            crate::arch::current::gdt::load_iopb(Some(&*iopb_ptr));
        }
    }

    // Capture saved-state pointers before releasing the lock.
    let current_state: *mut crate::arch::current::context::SavedState = if current.is_null()
    {
        core::ptr::null_mut()
    }
    else
    {
        // SAFETY: current is a valid TCB; saved_state field is always valid.
        unsafe { core::ptr::addr_of_mut!((*current).saved_state) }
    };
    // SAFETY: next is a valid TCB; saved_state field is always valid.
    let next_state = unsafe { core::ptr::addr_of!((*next).saved_state) };

    // Release the lock before calling switch. The context switch changes RSP
    // to the next thread's kernel stack; the unlock_raw call must complete on
    // the current stack before that happens.
    // SAFETY: saved_flags was returned by the matching lock_raw above;
    // restores interrupt state and unlocks scheduler.
    unsafe {
        sched.lock.unlock_raw(saved_flags);
    }

    if !current_state.is_null()
    {
        // SAFETY: both current_state and next_state are valid SavedState pointers
        // on heap-allocated TCBs; kernel stacks are valid; interrupts are re-enabled.
        unsafe {
            switch(current_state, next_state);
        }
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
    // SAFETY: single-CPU system; CPU 0 scheduler is always valid.
    let sched =
        unsafe { &mut SCHEDULERS[crate::arch::current::cpu::current_cpu() as usize] };
    let current = sched.current;
    if current.is_null()
    {
        return;
    }
    // SAFETY: current is a valid TCB; slice_remaining field is always valid.
    let remaining = unsafe { (*current).slice_remaining };
    if remaining == 0
    {
        // Idle threads have slice_remaining = 0 and should not be preempted.
        return;
    }
    let new_remaining = remaining - 1;
    // SAFETY: current is a valid TCB; slice_remaining field is always valid.
    unsafe {
        (*current).slice_remaining = new_remaining;
    }
    if new_remaining == 0
    {
        // Reset slice for next run.
        // SAFETY: current is a valid TCB; slice_remaining field is always valid.
        unsafe {
            (*current).slice_remaining = TIME_SLICE_TICKS;
        }
        // SAFETY: called from interrupt handler; valid kernel stack.
        unsafe {
            schedule();
        }
    }
}

// ── user_thread_trampoline ────────────────────────────────────────────────────

/// Entry point for new user threads created via `SYS_CAP_CREATE_THREAD`.
///
/// `switch()` jumps here when the thread runs for the first time (instead of
/// returning to a previous `switch` call site). By the time execution reaches
/// here, `schedule()` has already:
/// 1. Set the current TCB via `set_current(next)`.
/// 2. Switched the address space via `(*next.address_space).activate()`.
/// 3. Updated the kernel trap stack pointer.
///
/// The thread's [`TrapFrame`] was written by `SYS_THREAD_CONFIGURE`. This
/// function simply retrieves it and calls `return_to_user`, which restores user
/// registers and executes `iretq` / `sret`. Never returns.
///
/// # Safety
/// Must only be called as a `switch()` return target (i.e., stored as
/// `saved_state.rip`/`saved_state.ra` in a newly created user TCB). The TCB's
/// `trap_frame` must be non-null and point to a valid, initialized `TrapFrame`.
#[cfg(not(test))]
pub(crate) unsafe extern "C" fn user_thread_trampoline() -> !
{
    // SAFETY: current_tcb is set by schedule() before switch() is called; returns valid TCB pointer.
    let tcb = unsafe { crate::syscall::current_tcb() };
    // SAFETY: tcb is a valid TCB pointer; trap_frame was set by sys_thread_configure and points
    // to a valid, initialized TrapFrame. The initial RSP for this function is set below the
    // TrapFrame (see sys_cap_create_thread: trampoline_rsp = kstack_top - tf_size - TRAMPOLINE_FRAME)
    // so this C function's stack frame does not overlap the TrapFrame.
    unsafe { crate::arch::current::context::return_to_user((*tcb).trap_frame) }
}

// ── enter ─────────────────────────────────────────────────────────────────────

/// Start executing the highest-priority ready thread and never return.
///
/// Called once at the end of kernel boot after the init TCB has been enqueued.
/// Dequeues the init thread, activates its address space, sets TSS RSP0 /
/// `SYSCALL_KERNEL_RSP`, builds an initial user-mode [`TrapFrame`] on its kernel
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
    // SAFETY: single-threaded boot; SCHEDULERS[0] is exclusively owned; tcb is validated
    // non-null before dereference; state field is always valid.
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

    // Disable interrupts before entering user mode.
    // On x86-64: no-op (interrupts are re-enabled by iretq/sysret flags).
    // On RISC-V: return_to_user sets sstatus to SPP=0/SPIE=1/SIE=0 before
    // sret; SIE is re-enabled atomically by sret. Disabling here prevents any
    // stray interrupt from arriving before return_to_user arms sscratch.
    // SAFETY: single-boot-thread; prevents race before user mode entry.
    unsafe {
        crate::arch::current::cpu::disable_interrupts();
    }

    // Set the kernel trap stack pointer before entering user mode so the first
    // ring-3 → ring-0 transition lands on the correct kernel stack.
    // On x86-64: writes TSS RSP0 + SYSCALL_KERNEL_RSP.
    // On RISC-V: writes PerCpuData::kernel_rsp (offset 8 from tp); trap_entry
    //   loads this to locate the kernel stack on U-mode entry.  sscratch is set
    //   to &PER_CPU by return_to_user just before sret.
    // SAFETY: single-boot-thread; kernel_stack_top is valid from init TCB.
    unsafe {
        crate::arch::current::cpu::set_kernel_trap_stack(kernel_stack_top);
    }

    // Build the initial user-mode TrapFrame on the init thread's kernel stack.
    // The frame sits just below kernel_stack_top.
    let tf_size = core::mem::size_of::<TrapFrame>() as u64;
    let tf_ptr: *mut TrapFrame = (kernel_stack_top - tf_size) as *mut _;

    // Zero the frame then populate the user-mode entry fields via TrapFrame
    // methods (arch-specific field names are hidden inside trap_frame.rs).
    // SAFETY: tf_ptr is within the allocated kernel stack (kernel_stack_top - tf_size);
    // init_tcb is a valid TCB; saved_state and TrapFrame methods ensure correct field access.
    unsafe {
        core::ptr::write_bytes(tf_ptr.cast::<u8>(), 0, tf_size as usize);
        (*tf_ptr).init_user(entry_point, INIT_STACK_TOP);
        // Forward the initial user argument (cap slot, etc.) stored in
        // saved_state at TCB creation via new_state(…, arg, …).
        let user_arg = init_tcb.saved_state.user_arg();
        if user_arg != 0
        {
            (*tf_ptr).set_arg0(user_arg);
        }
    }

    // Record the trap_frame pointer in the TCB so future trap handlers can
    // find the correct frame when the thread is running.
    init_tcb.trap_frame = tf_ptr;

    // Read init's page table root before entering the switch function.
    // SAFETY: init_tcb.address_space is non-null and valid, set up in main.rs Phase 9 init;
    // root_phys field is always valid.
    let root_phys = unsafe { (*init_tcb.address_space).root_phys };

    crate::kprintln!("sched: enter - handing control to init");

    // Activate init's address space and enter user mode.
    // `first_entry_to_user` handles the arch-specific sequence:
    //   x86-64: atomically switches CR3 and executes iretq from init's kernel stack.
    //   RISC-V: writes satp (sfence serializes), then executes sret.
    // SAFETY: root_phys is init's valid page-table root (from Phase 9 init address space);
    // tf_ptr points to a valid, initialized TrapFrame on init's kernel stack.
    unsafe {
        crate::arch::current::context::first_entry_to_user(root_phys, tf_ptr);
    }
}
