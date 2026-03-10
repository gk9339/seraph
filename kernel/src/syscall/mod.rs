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

pub mod ipc;

use crate::arch::current::trap_frame::TrapFrame;
use syscall::{
    SyscallError, SYS_DEBUG_LOG, SYS_IPC_CALL, SYS_IPC_RECV, SYS_IPC_REPLY, SYS_SIGNAL_SEND,
    SYS_SIGNAL_WAIT, SYS_THREAD_EXIT, SYS_THREAD_YIELD,
};

// ── Architecture-specific TrapFrame accessors ─────────────────────────────────

/// Extract the syscall number from the TrapFrame.
#[cfg(target_arch = "x86_64")]
fn syscall_nr(tf: &TrapFrame) -> u64
{
    tf.rax
}

#[cfg(target_arch = "riscv64")]
fn syscall_nr(tf: &TrapFrame) -> u64
{
    tf.a7
}

/// Set the syscall return value in the TrapFrame.
#[cfg(target_arch = "x86_64")]
fn set_return(tf: &mut TrapFrame, val: i64)
{
    tf.rax = val as u64;
}

#[cfg(target_arch = "riscv64")]
fn set_return(tf: &mut TrapFrame, val: i64)
{
    tf.a0 = val as u64;
}

/// Read argument `n` (0-indexed) from the TrapFrame.
#[cfg(target_arch = "x86_64")]
pub fn arg(tf: &TrapFrame, n: usize) -> u64
{
    match n
    {
        0 => tf.rdi,
        1 => tf.rsi,
        2 => tf.rdx,
        3 => tf.r10,
        4 => tf.r8,
        5 => tf.r9,
        _ => 0,
    }
}

#[cfg(target_arch = "riscv64")]
pub fn arg(tf: &TrapFrame, n: usize) -> u64
{
    match n
    {
        0 => tf.a0,
        1 => tf.a1,
        2 => tf.a2,
        3 => tf.a3,
        4 => tf.a4,
        5 => tf.a5,
        _ => 0,
    }
}

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

    let nr = syscall_nr(tf);

    let ret: Result<u64, SyscallError> = match nr
    {
        SYS_IPC_CALL => ipc::sys_ipc_call(tf),
        SYS_IPC_REPLY => ipc::sys_ipc_reply(tf),
        SYS_IPC_RECV => ipc::sys_ipc_recv(tf),
        SYS_SIGNAL_SEND => ipc::sys_signal_send(tf),
        SYS_SIGNAL_WAIT => ipc::sys_signal_wait(tf),
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

    set_return(tf, ret_val);
}

// ── Thread syscall handlers ───────────────────────────────────────────────────

/// SYS_THREAD_YIELD (21): voluntarily yield the CPU.
///
/// Phase 9: no-op — single-threaded, no preemption yet.
/// Phase 10: invoke the scheduler to pick the next thread.
#[cfg(not(test))]
fn sys_yield(_tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    // TODO Phase 10: call schedule() to context-switch to the next thread.
    Ok(0)
}

/// SYS_THREAD_EXIT (22): terminate the calling thread.
///
/// Phase 9: print a diagnostic and halt. A real implementation would free
/// the TCB and context-switch to the next runnable thread.
#[cfg(not(test))]
fn sys_exit(_tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    crate::kprintln!("init: SYS_THREAD_EXIT called — halting");
    crate::arch::current::cpu::halt_loop();
}

// ── Debug log ─────────────────────────────────────────────────────────────────

/// SYS_DEBUG_LOG (44): write a UTF-8 string to the kernel console.
///
/// Arguments: arg0 = pointer to string data (user virtual address),
///            arg1 = byte length (clamped to 1024).
///
/// Phase 9 simplification: reads user memory directly (no SMAP; same address
/// space as the caller since we don't switch CR3/satp on syscall entry).
/// Production: validate the pointer range against the caller's address space.
#[cfg(not(test))]
fn sys_debug_log(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    let ptr = arg(tf, 0) as *const u8;
    let len = (arg(tf, 1) as usize).min(1024);

    if ptr.is_null()
    {
        return Err(SyscallError::InvalidArgument);
    }

    // SAFETY: Phase 9 — same page tables active in kernel; user VA is mapped.
    // No SMAP enforcement. Length is clamped above.
    let slice = unsafe { core::slice::from_raw_parts(ptr, len) };

    match core::str::from_utf8(slice)
    {
        Ok(s) => crate::kprintln!("[init] {}", s),
        Err(_) => crate::kprintln!("[init] <{} non-UTF-8 bytes>", len),
    }

    Ok(0)
}
