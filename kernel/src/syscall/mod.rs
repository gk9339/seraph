// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/syscall/mod.rs

//! Kernel syscall dispatch.
//!
//! `dispatch` is called from the architecture-specific trap/syscall entry
//! with a pointer to the current thread's [`TrapFrame`]. It reads the syscall
//! number, calls the appropriate handler, and writes the return value back.
//!
//! # Syscall ABI
//! - x86-64: number in `rax`; args in `rdi`/`rsi`/`rdx`/`r10`/`r8`/`r9`;
//!   return value in `rax`.
//! - RISC-V: number in `a7`; args in `a0`–`a5`; return value in `a0`.
//!
//! # Adding new syscalls
//! 1. Add the constant to `abi/syscall/src/lib.rs`.
//! 2. Add a handler function in the appropriate `syscall/` submodule.
//! 3. Add a `match` arm in `dispatch` below.

#[cfg(not(test))]
extern crate alloc;

pub mod cap;
pub mod hw;
pub mod ipc;
pub mod mem;
pub mod sysinfo;
pub mod thread;

use crate::arch::current::trap_frame::TrapFrame;
#[cfg(not(test))]
use syscall::SyscallError;

#[cfg(not(test))]
use syscall::{
    SYS_ASPACE_QUERY, SYS_CAP_COPY, SYS_CAP_CREATE_ASPACE, SYS_CAP_CREATE_CSPACE,
    SYS_CAP_CREATE_ENDPOINT, SYS_CAP_CREATE_EVENT_Q, SYS_CAP_CREATE_SIGNAL, SYS_CAP_CREATE_THREAD,
    SYS_CAP_CREATE_WAIT_SET, SYS_CAP_DELETE, SYS_CAP_DERIVE, SYS_CAP_INSERT, SYS_CAP_MOVE,
    SYS_CAP_REVOKE, SYS_DEBUG_LOG, SYS_DMA_GRANT, SYS_EVENT_POST, SYS_EVENT_RECV, SYS_FRAME_SPLIT,
    SYS_IOPORT_BIND, SYS_IPC_BUFFER_SET, SYS_IPC_CALL, SYS_IPC_RECV, SYS_IPC_REPLY, SYS_IRQ_ACK,
    SYS_IRQ_REGISTER, SYS_MEM_MAP, SYS_MEM_PROTECT, SYS_MEM_UNMAP, SYS_MMIO_MAP, SYS_SIGNAL_SEND,
    SYS_SIGNAL_WAIT, SYS_SYSTEM_INFO, SYS_THREAD_CONFIGURE, SYS_THREAD_EXIT, SYS_THREAD_READ_REGS,
    SYS_THREAD_SET_AFFINITY, SYS_THREAD_SET_PRIORITY, SYS_THREAD_START, SYS_THREAD_STOP,
    SYS_THREAD_WRITE_REGS, SYS_THREAD_YIELD, SYS_WAIT_SET_ADD, SYS_WAIT_SET_REMOVE,
    SYS_WAIT_SET_WAIT,
};

// ── TrapFrame accessor shims ──────────────────────────────────────────────────
// `TrapFrame::syscall_nr`, `::set_return`, and `::arg` are defined as methods
// on `TrapFrame` in each arch's `trap_frame.rs`. Callers (ipc.rs etc.) use
// `tf.arg(n)` directly; this comment marks where the free functions used to
// live so the removal is easy to trace.

// ── Syscall dispatch ──────────────────────────────────────────────────────────

/// Syscall dispatcher: called from the arch-specific trap/syscall entry stub.
///
/// Reads the syscall number from `tf`, invokes the handler, and writes the
/// return value (or negative error code) back into `tf`.
///
/// # Safety
/// `tf` must point to a valid, kernel-stack-resident [`TrapFrame`] for the
/// currently running thread. The caller must have already switched to the
/// kernel stack.
#[cfg(not(test))]
pub unsafe fn dispatch(tf: *mut TrapFrame)
{
    // SAFETY: caller guarantees tf is valid and exclusively accessible.
    let tf = unsafe { &mut *tf };

    let nr = tf.syscall_nr();

    let ret: Result<u64, SyscallError> = match nr
    {
        SYS_IPC_CALL => ipc::sys_ipc_call(tf),
        SYS_IPC_REPLY => ipc::sys_ipc_reply(tf),
        SYS_IPC_RECV => ipc::sys_ipc_recv(tf),
        SYS_SIGNAL_SEND => ipc::sys_signal_send(tf),
        SYS_SIGNAL_WAIT => ipc::sys_signal_wait(tf),
        SYS_CAP_CREATE_ENDPOINT => cap::sys_cap_create_endpoint(tf),
        SYS_CAP_CREATE_SIGNAL => cap::sys_cap_create_signal(tf),
        SYS_CAP_CREATE_ASPACE => cap::sys_cap_create_aspace(tf),
        SYS_CAP_CREATE_CSPACE => cap::sys_cap_create_cspace(tf),
        SYS_CAP_CREATE_THREAD => cap::sys_cap_create_thread(tf),
        SYS_CAP_COPY => cap::sys_cap_copy(tf),
        SYS_CAP_DERIVE => cap::sys_cap_derive(tf),
        SYS_CAP_DELETE => cap::sys_cap_delete(tf),
        SYS_CAP_REVOKE => cap::sys_cap_revoke(tf),
        SYS_CAP_MOVE => cap::sys_cap_move(tf),
        SYS_CAP_INSERT => cap::sys_cap_insert(tf),
        SYS_MEM_MAP => mem::sys_mem_map(tf),
        SYS_MEM_UNMAP => mem::sys_mem_unmap(tf),
        SYS_MEM_PROTECT => mem::sys_mem_protect(tf),
        SYS_FRAME_SPLIT => mem::sys_frame_split(tf),
        SYS_THREAD_CONFIGURE => thread::sys_thread_configure(tf),
        SYS_THREAD_START => thread::sys_thread_start(tf),
        SYS_THREAD_STOP => thread::sys_thread_stop(tf),
        SYS_THREAD_SET_PRIORITY => thread::sys_thread_set_priority(tf),
        SYS_THREAD_SET_AFFINITY => thread::sys_thread_set_affinity(tf),
        SYS_THREAD_READ_REGS => thread::sys_thread_read_regs(tf),
        SYS_THREAD_WRITE_REGS => thread::sys_thread_write_regs(tf),
        SYS_IPC_BUFFER_SET => sys_ipc_buffer_set(tf),
        SYS_THREAD_YIELD => sys_yield(tf),
        SYS_THREAD_EXIT => sys_exit(tf),
        SYS_DEBUG_LOG => sys_debug_log(tf),
        SYS_SYSTEM_INFO => sysinfo::sys_system_info(tf),
        SYS_ASPACE_QUERY => sysinfo::sys_aspace_query(tf),
        SYS_EVENT_POST => ipc::sys_event_post(tf),
        SYS_EVENT_RECV => ipc::sys_event_recv(tf),
        SYS_CAP_CREATE_EVENT_Q => cap::sys_cap_create_event_queue(tf),
        SYS_CAP_CREATE_WAIT_SET => cap::sys_cap_create_wait_set(tf),
        SYS_WAIT_SET_ADD => ipc::sys_wait_set_add(tf),
        SYS_WAIT_SET_REMOVE => ipc::sys_wait_set_remove(tf),
        SYS_WAIT_SET_WAIT => ipc::sys_wait_set_wait(tf),
        SYS_IRQ_ACK => hw::sys_irq_ack(tf),
        SYS_IRQ_REGISTER => hw::sys_irq_register(tf),
        SYS_MMIO_MAP => hw::sys_mmio_map(tf),
        SYS_IOPORT_BIND => hw::sys_ioport_bind(tf),
        SYS_DMA_GRANT => hw::sys_dma_grant(tf),
        _ => Err(SyscallError::UnknownSyscall),
    };

    let ret_val = match ret
    {
        Ok(v) => v.cast_signed(),
        Err(e) => e as i64,
    };

    tf.set_return(ret_val);
}

// ── Thread syscall handlers ───────────────────────────────────────────────────

/// `SYS_THREAD_YIELD` (21): voluntarily yield the CPU.
// unnecessary_wraps: all dispatch arms must return Result<u64, SyscallError>; signature is fixed.
#[cfg(not(test))]
#[allow(clippy::unnecessary_wraps)]
fn sys_yield(_tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    // SAFETY: called from syscall handler on a valid kernel stack.
    unsafe {
        crate::sched::schedule();
    }
    Ok(0)
}

/// `SYS_THREAD_EXIT` (22): terminate the calling thread.
///
/// Marks the current thread as `Exited` and calls `schedule()` to switch
/// to the next runnable thread. The exited thread is never re-enqueued.
///
/// Note: full resource cleanup (freeing kernel stack, TCB, `CSpace` entries)
/// requires a process-manager teardown path that does not yet exist. For now
/// the TCB is abandoned in place — it will not be scheduled again.
#[cfg(not(test))]
fn sys_exit(_tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    use crate::sched::thread::ThreadState;
    // SAFETY: current_tcb() returns current thread; interrupt context ensures it is set.
    let tcb = unsafe { current_tcb() };
    if !tcb.is_null()
    {
        // SAFETY: tcb validated non-null; state field always valid for initialized TCB.
        unsafe {
            (*tcb).state = ThreadState::Exited;
        }
    }
    // Switch to the next runnable thread. The exited thread is in Exited state
    // so schedule() will not re-enqueue it.
    // SAFETY: called from syscall handler on a valid kernel stack.
    unsafe {
        crate::sched::schedule();
    }
    // schedule() returns here if the same thread is re-selected (shouldn't
    // happen for an Exited thread, but halt as a safety net).
    crate::arch::current::cpu::halt_loop();
}

// ── Scheduler / IPC helpers ───────────────────────────────────────────────────

/// Get the current thread's TCB pointer (BSP only; single-CPU until WSMP).
///
/// # Safety
/// Must be called from a kernel context. The current CPU's scheduler must have
/// been initialised by `sched::init`.
#[cfg(not(test))]
pub(crate) unsafe fn current_tcb() -> *mut crate::sched::thread::ThreadControlBlock
{
    let cpu = crate::arch::current::cpu::current_cpu() as usize;
    // SAFETY: SCHEDULERS[cpu] is initialised for all online CPUs; per-CPU data indexed by CPU ID.
    unsafe { crate::sched::scheduler_for(cpu).current }
}

/// Look up a capability slot in a `CSpace` by index.
///
/// Returns a reference to the slot, or an appropriate [`SyscallError`]:
/// - Null cspace pointer or missing slot → [`SyscallError::InvalidCapability`].
/// - Tag mismatch → [`SyscallError::InvalidCapability`].
/// - Insufficient rights → [`SyscallError::InsufficientRights`].
#[cfg(not(test))]
pub(crate) unsafe fn lookup_cap(
    cspace: *mut crate::cap::cspace::CSpace,
    index: u32,
    expected_tag: crate::cap::slot::CapTag,
    required_rights: crate::cap::slot::Rights,
) -> Result<&'static crate::cap::slot::CapabilitySlot, SyscallError>
{
    if cspace.is_null()
    {
        return Err(SyscallError::InvalidCapability);
    }
    // SAFETY: cspace is a valid CSpace pointer from the current thread's TCB.
    let cs = unsafe { &*cspace };
    let slot = cs.slot(index).ok_or(SyscallError::InvalidCapability)?;
    if slot.tag != expected_tag
    {
        return Err(SyscallError::InvalidCapability);
    }
    if !slot.rights.contains(required_rights)
    {
        return Err(SyscallError::InsufficientRights);
    }
    // ref_as_ptr: intentional — raw pointer cast to extend lifetime to 'static.
    #[allow(clippy::ref_as_ptr)]
    // SAFETY: slot lives in the CSpace which is heap-allocated for the lifetime of the process.
    Ok(unsafe { &*(slot as *const _) })
}

// ── IPC buffer ────────────────────────────────────────────────────────────────

/// `SYS_IPC_BUFFER_SET` (42): register (or clear) the per-thread IPC buffer page.
///
/// arg0 = virtual address of the IPC buffer page (0 = deregister; otherwise
/// must be 4 KiB-aligned).
#[cfg(not(test))]
fn sys_ipc_buffer_set(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    let virt = tf.arg(0);
    if virt != 0 && (virt & 0xFFF) != 0
    {
        return Err(SyscallError::InvalidAddress);
    }
    // SAFETY: current_tcb() is valid from a syscall context.
    let tcb = unsafe { current_tcb() };
    if tcb.is_null()
    {
        return Err(SyscallError::InvalidCapability);
    }
    // SAFETY: tcb is a valid TCB.
    unsafe {
        (*tcb).ipc_buffer = virt;
    }
    Ok(0)
}

// ── Debug log ─────────────────────────────────────────────────────────────────

// TODO: remove SYS_DEBUG_LOG once logd is running and init uses IPC logging.

/// `SYS_DEBUG_LOG` (44): write a UTF-8 string to the kernel console.
///
/// **TEMPORARY** — this syscall is a development scaffold for use before
/// `logd` is available. It reads user memory directly and prints via the
/// kernel's own console (`kprintln!`). The correct production path is for
/// userspace to log via IPC to `logd`; this syscall will be removed once
/// that path is implemented (W11). It must not be used in production code.
///
/// Arguments: arg0 = pointer to string data (user virtual address),
///            arg1 = byte length (clamped to 1024).
///
/// Removed once logd is running and init uses IPC logging (W11).
// cast_possible_truncation: Seraph is 64-bit only; usize == u64 at runtime.
#[cfg(not(test))]
#[allow(clippy::cast_possible_truncation)]
fn sys_debug_log(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    let ptr = tf.arg(0) as *const u8;
    let len = (tf.arg(1) as usize).min(1024);

    if ptr.is_null()
    {
        return Err(SyscallError::InvalidArgument);
    }

    // Copy user bytes into a kernel stack buffer before inspecting them.
    // Bracket with user_access_begin/end to satisfy SMAP (x86-64) / SUM (RISC-V).
    let mut buf = [0u8; 1024];
    // SAFETY: user_access_begin/end bracket user memory copy; ptr validated non-null; len clamped to buf size.
    unsafe {
        crate::arch::current::cpu::user_access_begin();
        core::ptr::copy_nonoverlapping(ptr, buf.as_mut_ptr(), len);
        crate::arch::current::cpu::user_access_end();
    }

    if let Ok(s) = core::str::from_utf8(&buf[..len])
    {
        crate::kprintln!("[init] {}", s);
    }
    else
    {
        crate::kprintln!("[init] <{} non-UTF-8 bytes>", len);
    }

    Ok(0)
}
