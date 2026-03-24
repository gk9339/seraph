// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// shared/syscall/src/lib.rs

//! Raw syscall wrappers for Seraph userspace.
//!
//! Thin `no_std`-compatible functions that issue the architecture-specific
//! syscall instruction (`SYSCALL` on x86-64, `ECALL` on RISC-V) and return
//! the kernel result.
//!
//! IPC calls that transfer data words require the caller to have registered an
//! IPC buffer page via [`ipc_buffer_set`] first.
//!
//! # ABI
//! - x86-64: syscall number in `rax`; args in `rdi/rsi/rdx/r10/r8/r9`;
//!   return in `rax` (primary), `rdx` (secondary label for ipc_call/ipc_recv).
//! - RISC-V: syscall number in `a7`; args in `a0–a5`;
//!   return in `a0` (primary), `a1` (secondary label).

#![no_std]

use syscall_abi::{
    SYS_CAP_CREATE_ENDPOINT, SYS_CAP_CREATE_SIGNAL, SYS_DEBUG_LOG, SYS_IPC_BUFFER_SET,
    SYS_IPC_CALL, SYS_IPC_RECV, SYS_IPC_REPLY, SYS_SIGNAL_SEND, SYS_SIGNAL_WAIT,
    SYS_THREAD_EXIT, SYS_THREAD_YIELD, MSG_DATA_WORDS_MAX,
};

// ── Raw syscall entry ─────────────────────────────────────────────────────────

/// Issue a syscall with up to 2 arguments. Returns the primary return value.
#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn syscall2(nr: u64, a0: u64, a1: u64) -> i64
{
    let ret: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            inout("rax") nr as i64 => ret,
            in("rdi") a0,
            in("rsi") a1,
            // syscall clobbers rcx and r11.
            out("rcx") _,
            out("r11") _,
            options(nostack),
        );
    }
    ret
}

#[cfg(target_arch = "riscv64")]
#[inline(always)]
unsafe fn syscall2(nr: u64, a0: u64, a1: u64) -> i64
{
    let ret: i64;
    unsafe {
        core::arch::asm!(
            "ecall",
            inout("a0") a0 as i64 => ret,
            in("a1") a1,
            in("a7") nr,
            options(nostack),
        );
    }
    ret
}

/// Issue a syscall with up to 3 arguments.
#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn syscall3(nr: u64, a0: u64, a1: u64, a2: u64) -> i64
{
    let ret: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            inout("rax") nr as i64 => ret,
            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            out("rcx") _,
            out("r11") _,
            options(nostack),
        );
    }
    ret
}

#[cfg(target_arch = "riscv64")]
#[inline(always)]
unsafe fn syscall3(nr: u64, a0: u64, a1: u64, a2: u64) -> i64
{
    let ret: i64;
    unsafe {
        core::arch::asm!(
            "ecall",
            inout("a0") a0 as i64 => ret,
            in("a1") a1,
            in("a2") a2,
            in("a7") nr,
            options(nostack),
        );
    }
    ret
}

/// Issue a syscall with up to 5 arguments. Returns (primary, secondary).
#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn syscall5_ret2(nr: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> (i64, u64)
{
    let ret: i64;
    let secondary: u64;
    unsafe {
        core::arch::asm!(
            "syscall",
            inout("rax") nr as i64 => ret,
            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            in("r10") a3,
            in("r8")  a4,
            out("rcx") _,
            out("r11") _,
            lateout("rdx") secondary,
            options(nostack),
        );
    }
    (ret, secondary)
}

#[cfg(target_arch = "riscv64")]
#[inline(always)]
unsafe fn syscall5_ret2(nr: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> (i64, u64)
{
    let ret: i64;
    let secondary: u64;
    unsafe {
        core::arch::asm!(
            "ecall",
            inout("a0") a0 as i64 => ret,
            inout("a1") a1 => secondary,
            in("a2") a2,
            in("a3") a3,
            in("a4") a4,
            in("a7") nr,
            options(nostack),
        );
    }
    (ret, secondary)
}

// ── Public syscall wrappers ───────────────────────────────────────────────────

/// Write a UTF-8 string to the kernel console.
///
/// **TEMPORARY — do not use in production code.**
///
/// This is a thin wrapper around `SYS_DEBUG_LOG`, a development scaffold
/// that exists only until `logd` and the IPC logging path are available.
/// It will be removed once userspace can log via `logd`. Use it only in
/// early-boot / Phase 10 test programs.
#[inline]
pub fn debug_log(msg: &str) -> Result<(), i64>
{
    let ret = unsafe { syscall2(SYS_DEBUG_LOG, msg.as_ptr() as u64, msg.len() as u64) };
    if ret < 0 { Err(ret) } else { Ok(()) }
}

/// Voluntarily yield the CPU to the next runnable thread.
#[inline]
pub fn thread_yield() -> Result<(), i64>
{
    let ret = unsafe { syscall2(SYS_THREAD_YIELD, 0, 0) };
    if ret < 0 { Err(ret) } else { Ok(()) }
}

/// Exit the current thread. Never returns.
#[inline]
pub fn thread_exit() -> !
{
    unsafe { syscall2(SYS_THREAD_EXIT, 0, 0) };
    // The syscall never returns; loop to satisfy the diverging type.
    loop { core::hint::spin_loop(); }
}

/// Register (or clear) the per-thread IPC buffer page.
///
/// `virt` must be 4 KiB-aligned (or 0 to deregister).
#[inline]
pub fn ipc_buffer_set(virt: u64) -> Result<(), i64>
{
    let ret = unsafe { syscall2(SYS_IPC_BUFFER_SET, virt, 0) };
    if ret < 0 { Err(ret) } else { Ok(()) }
}

/// Synchronous IPC call on an endpoint cap.
///
/// Sends `label` and up to `MSG_DATA_WORDS_MAX` data words (written to the
/// IPC buffer before the call). Blocks until a server replies.
/// Returns `(reply_label, reply_data_count)`.
///
/// # Note
/// The caller must have registered an IPC buffer via [`ipc_buffer_set`].
/// The reply data words are read from the same buffer after the call returns.
#[inline]
pub fn ipc_call(ep: u32, label: u64, data_count: usize) -> Result<(u64, usize), i64>
{
    let (ret, secondary) = unsafe {
        syscall5_ret2(
            SYS_IPC_CALL,
            ep as u64,
            label,
            data_count as u64,
            0, // cap_slots — ignored Phase 10
            0, // flags
        )
    };
    if ret < 0 { Err(ret) } else { Ok((secondary, 0)) }
}

/// Receive a call on an endpoint cap.
///
/// Blocks until a caller sends. Returns `(label, data_count)`.
/// Data words are written to the registered IPC buffer.
#[inline]
pub fn ipc_recv(ep: u32) -> Result<(u64, usize), i64>
{
    let (ret, secondary) = unsafe { syscall5_ret2(SYS_IPC_RECV, ep as u64, 0, 0, 0, 0) };
    if ret < 0 { Err(ret) } else { Ok((secondary, 0)) }
}

/// Reply to the thread that called us.
///
/// Sends `label` and `data_count` words from the IPC buffer back to the caller.
#[inline]
pub fn ipc_reply(label: u64, data_count: usize) -> Result<(), i64>
{
    let ret = unsafe { syscall3(SYS_IPC_REPLY, label, data_count as u64, 0) };
    if ret < 0 { Err(ret) } else { Ok(()) }
}

/// Send `bits` to a signal cap. `bits` must be non-zero.
#[inline]
pub fn signal_send(sig: u32, bits: u64) -> Result<(), i64>
{
    let ret = unsafe { syscall2(SYS_SIGNAL_SEND, sig as u64, bits) };
    if ret < 0 { Err(ret) } else { Ok(()) }
}

/// Block until any bits are set on a signal cap. Returns the acquired bitmask.
#[inline]
pub fn signal_wait(sig: u32) -> Result<u64, i64>
{
    let ret = unsafe { syscall2(SYS_SIGNAL_WAIT, sig as u64, 0) };
    if ret < 0 { Err(ret) } else { Ok(ret as u64) }
}

/// Create a new Endpoint object. Returns the CSpace slot index.
#[inline]
pub fn cap_create_endpoint() -> Result<u32, i64>
{
    let ret = unsafe { syscall2(SYS_CAP_CREATE_ENDPOINT, 0, 0) };
    if ret < 0 { Err(ret) } else { Ok(ret as u32) }
}

/// Create a new Signal object. Returns the CSpace slot index.
#[inline]
pub fn cap_create_signal() -> Result<u32, i64>
{
    let ret = unsafe { syscall2(SYS_CAP_CREATE_SIGNAL, 0, 0) };
    if ret < 0 { Err(ret) } else { Ok(ret as u32) }
}

// Silence "unused import" if user only uses some functions.
const _: usize = MSG_DATA_WORDS_MAX;
