// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/sched/mod.rs

//! Kernel scheduler — Phase 8/9: per-CPU state, idle threads, and init launch.
//!
//! Phase 8 allocates a kernel stack and idle TCB for each CPU.
//! Phase 9 adds `enter()`, which dequeues the init thread, builds its initial
//! user-mode [`TrapFrame`], activates its address space, and calls
//! `return_to_user` to start init running.
//!
//! # Deferred work
//! - Phase 10: SMP bringup, secondary CPU idle threads, load balancing,
//!   real context switching between multiple threads.

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
        &mut *tcb
    };

    let kernel_stack_top = init_tcb.kernel_stack_top;

    // Retrieve the user entry point stored in saved_state at TCB creation.
    #[cfg(target_arch = "x86_64")]
    let entry_point = init_tcb.saved_state.rip;
    #[cfg(target_arch = "riscv64")]
    let entry_point = init_tcb.saved_state.ra;

    // x86-64: update TSS RSP0 and SYSCALL_KERNEL_RSP so the next ring-3 →
    // ring-0 transition (interrupt or SYSCALL) lands on the right kernel stack.
    #[cfg(target_arch = "x86_64")]
    unsafe {
        crate::arch::current::gdt::set_rsp0(kernel_stack_top);
        crate::arch::current::syscall::set_kernel_rsp(kernel_stack_top);
    }

    // Build the initial user-mode TrapFrame on the init thread's kernel stack.
    // The frame sits just below kernel_stack_top.
    let tf_size = core::mem::size_of::<TrapFrame>() as u64;
    let tf_ptr = (kernel_stack_top - tf_size) as *mut TrapFrame;

    // Zero the frame then fill in the non-zero fields.
    // SAFETY: kernel_stack_top - tf_size is within the allocated kernel stack.
    unsafe {
        core::ptr::write_bytes(tf_ptr as *mut u8, 0, tf_size as usize);

        // x86-64: user entry via iretq.
        #[cfg(target_arch = "x86_64")]
        {
            (*tf_ptr).rip    = entry_point;
            (*tf_ptr).rsp    = INIT_STACK_TOP;
            (*tf_ptr).cs     = 0x23; // USER_CS (ring 3)
            (*tf_ptr).ss     = 0x1B; // USER_DS (ring 3)
            (*tf_ptr).rflags = 0x202; // IF=1, reserved bit 1 always set
        }

        // RISC-V: user entry via sret (sepc + sstatus set in return_to_user).
        #[cfg(target_arch = "riscv64")]
        {
            (*tf_ptr).sepc = entry_point;
            (*tf_ptr).sp   = INIT_STACK_TOP; // x2 = user stack pointer
        }
    }

    // Record the trap_frame pointer in the TCB so future trap handlers can
    // find the correct frame when the thread is running.
    init_tcb.trap_frame = tf_ptr;

    // Read init's page table root before entering the switch function.
    // SAFETY: init_tcb.address_space was set up in main.rs Phase 9 init.
    let root_phys = unsafe { (*init_tcb.address_space).root_phys };

    crate::kprintln!("  sched::enter: handing control to init");

    // On x86-64: atomically switch page tables and enter user mode.
    // switch_and_enter_user switches RSP to init's kernel stack BEFORE writing
    // CR3, so no Rust call/return happens on the boot stack after the CR3
    // write. The boot stack's identity mapping lives in PML4 entries 0–255
    // (lower half) which are not present in the init address space.
    //
    // On RISC-V: activate first (satp write + sfence.vma does not require a
    // return, as the sfence serializes execution), then return_to_user.
    // The RISC-V boot stack is in physical RAM covered by the direct map
    // (PPN entries 256–511), so it remains accessible after satp write.
    #[cfg(target_arch = "x86_64")]
    unsafe {
        use crate::arch::current::context::switch_and_enter_user;
        switch_and_enter_user(root_phys, tf_ptr);
    }

    #[cfg(target_arch = "riscv64")]
    unsafe {
        use crate::arch::current::context::return_to_user;
        (*init_tcb.address_space).activate();
        return_to_user(tf_ptr)
    }

    // Unreachable: both paths above diverge.
    #[allow(unreachable_code)]
    loop {}
}
