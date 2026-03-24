// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/syscall/mod.rs

//! Kernel syscall dispatch (Phase 9).
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
pub mod ipc;

use crate::arch::current::trap_frame::TrapFrame;
#[cfg(not(test))]
use syscall::SyscallError;

#[cfg(not(test))]
use syscall::{
    SYS_CAP_CREATE_ENDPOINT, SYS_CAP_CREATE_SIGNAL, SYS_DEBUG_LOG, SYS_IPC_BUFFER_SET,
    SYS_IPC_CALL, SYS_IPC_RECV, SYS_IPC_REPLY, SYS_SIGNAL_SEND, SYS_SIGNAL_WAIT, SYS_THREAD_EXIT,
    SYS_THREAD_YIELD,
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
        SYS_IPC_BUFFER_SET => sys_ipc_buffer_set(tf),
        SYS_THREAD_YIELD => sys_yield(tf),
        SYS_THREAD_EXIT => sys_exit(tf),
        SYS_DEBUG_LOG => sys_debug_log(tf),
        _ => Err(SyscallError::UnknownSyscall),
    };

    let ret_val = match ret
    {
        Ok(v) => v as i64,
        Err(e) => e as i64,
    };

    tf.set_return(ret_val);
}

// ── Thread syscall handlers ───────────────────────────────────────────────────

/// SYS_THREAD_YIELD (21): voluntarily yield the CPU.
#[cfg(not(test))]
fn sys_yield(_tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    // SAFETY: called from syscall handler on a valid kernel stack.
    unsafe { crate::sched::schedule(); }
    Ok(0)
}

/// SYS_THREAD_EXIT (22): terminate the calling thread.
///
/// Phase 9: print a diagnostic and halt. A real implementation would free
/// the TCB and context-switch to the next runnable thread.
#[cfg(not(test))]
fn sys_exit(_tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    crate::kprintln!("init: SYS_THREAD_EXIT called - halting");
    crate::arch::current::cpu::halt_loop();
}

// ── Scheduler / IPC helpers ───────────────────────────────────────────────────

/// Get the current thread's TCB pointer (BSP only, Phase 10).
///
/// # Safety
/// Must be called from a kernel context. SCHEDULERS[0] must have been
/// initialised by `sched::init`.
#[cfg(not(test))]
pub(crate) unsafe fn current_tcb() -> *mut crate::sched::thread::ThreadControlBlock
{
    // SAFETY: SCHEDULERS[0] is initialised; single-CPU Phase 10.
    unsafe { crate::sched::scheduler_for(0).current }
}

/// Look up a capability slot in a CSpace by index.
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
    // SAFETY: slot lives in the CSpace which is heap-allocated for the
    // lifetime of the process.
    Ok(unsafe { &*(slot as *const _) })
}

// ── IPC buffer ────────────────────────────────────────────────────────────────

/// SYS_IPC_BUFFER_SET (42): register (or clear) the per-thread IPC buffer page.
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
    unsafe { (*tcb).ipc_buffer = virt; }
    Ok(0)
}

// ── Debug log ─────────────────────────────────────────────────────────────────

// TODO: remove SYS_DEBUG_LOG once logd is running and init uses IPC logging.

/// SYS_DEBUG_LOG (44): write a UTF-8 string to the kernel console.
///
/// **TEMPORARY** — this syscall is a development scaffold for use before
/// `logd` is available. It reads user memory directly and prints via the
/// kernel's own console (`kprintln!`). The correct production path is for
/// userspace to log via IPC to `logd`; this syscall will be removed once
/// that path is implemented. It must not be used in code that is intended
/// to survive past Phase 10.
///
/// Arguments: arg0 = pointer to string data (user virtual address),
///            arg1 = byte length (clamped to 1024).
#[cfg(not(test))]
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
    unsafe {
        crate::arch::current::cpu::user_access_begin();
        core::ptr::copy_nonoverlapping(ptr, buf.as_mut_ptr(), len);
        crate::arch::current::cpu::user_access_end();
    }

    match core::str::from_utf8(&buf[..len])
    {
        Ok(s) => crate::kprintln!("[init] {}", s),
        Err(_) => crate::kprintln!("[init] <{} non-UTF-8 bytes>", len),
    }

    Ok(0)
}
