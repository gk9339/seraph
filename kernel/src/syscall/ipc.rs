// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/syscall/ipc.rs

//! IPC syscall handlers — Phase 10.
//!
//! All handlers look up the target capability in the current thread's CSpace,
//! call the corresponding IPC kernel function, and enqueue/dequeue threads
//! via the scheduler as needed.
//!
//! Data words (up to MSG_DATA_WORDS_MAX) are read from / written to the
//! per-thread IPC buffer page registered via SYS_IPC_BUFFER_SET.

#[cfg(not(test))]
extern crate alloc;

#[cfg(not(test))]
use crate::arch::current::trap_frame::TrapFrame;
#[cfg(not(test))]
use syscall::SyscallError;

#[cfg(not(test))]
use crate::cap::slot::{CapTag, Rights};
#[cfg(not(test))]
use crate::ipc::message::Message;
#[cfg(not(test))]
use syscall::MSG_DATA_WORDS_MAX;
#[cfg(not(test))]
use super::{current_tcb, lookup_cap};

// ── IPC buffer helpers ────────────────────────────────────────────────────────

/// Read up to `count` data words from the thread's IPC buffer into `dst`.
///
/// Returns `Err(InvalidArgument)` if the buffer is not registered.
///
/// # Safety
/// Phase 10 simplification: user VA is accessible from the kernel because
/// the same page tables are active. Production would validate the mapping.
#[cfg(not(test))]
unsafe fn read_ipc_buf(buf: u64, count: usize, dst: &mut [u64; MSG_DATA_WORDS_MAX]) -> Result<(), SyscallError>
{
    if buf == 0
    {
        return Err(SyscallError::InvalidArgument);
    }
    let ptr = buf as *const u64;
    for i in 0..count
    {
        // SAFETY: buf is page-aligned and user-mapped; count <= MSG_DATA_WORDS_MAX.
        dst[i] = unsafe { core::ptr::read_volatile(ptr.add(i)) };
    }
    Ok(())
}

/// Write up to `count` data words from `src` to the thread's IPC buffer.
///
/// Silently skips if buffer is not registered (caller may not want data).
///
/// # Safety
/// Same Phase 10 simplification as `read_ipc_buf`.
#[cfg(not(test))]
unsafe fn write_ipc_buf(buf: u64, count: usize, src: &[u64; MSG_DATA_WORDS_MAX])
{
    if buf == 0 || count == 0
    {
        return;
    }
    let ptr = buf as *mut u64;
    for i in 0..count
    {
        // SAFETY: buf is page-aligned and user-mapped; count <= MSG_DATA_WORDS_MAX.
        unsafe { core::ptr::write_volatile(ptr.add(i), src[i]); }
    }
}

// ── IPC syscall handlers ──────────────────────────────────────────────────────

/// SYS_IPC_CALL (0): synchronous call on an endpoint.
///
/// arg0 = endpoint cap index, arg1 = label, arg2 = data_count,
/// arg3 = cap_slots (ignored Phase 10), arg4 = flags (ignored Phase 10).
///
/// Blocks caller until a server replies. On return, label and data words from
/// the reply are available in the return registers and IPC buffer.
#[cfg(not(test))]
pub fn sys_ipc_call(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    let ep_idx = tf.arg(0) as u32;
    let label  = tf.arg(1);
    let data_count = (tf.arg(2) as usize).min(MSG_DATA_WORDS_MAX);

    // SAFETY: current_tcb() valid from syscall context.
    let tcb = unsafe { current_tcb() };
    if tcb.is_null()
    {
        return Err(SyscallError::InvalidCapability);
    }

    // SAFETY: tcb is valid.
    let _cspace_ptr = unsafe { (*tcb).cspace };

    let slot = unsafe { lookup_cap(_cspace_ptr, ep_idx, CapTag::Endpoint, Rights::SEND) }?;

    // Extract EndpointState pointer from the slot's object.
    // SAFETY: slot.object is a NonNull<KernelObjectHeader> at offset 0 of
    // EndpointObject; EndpointObject.state is valid.
    let ep_state = unsafe {
        let obj_ptr = slot.object.ok_or(SyscallError::InvalidCapability)?.as_ptr();
        let ep_obj = obj_ptr as *mut crate::cap::object::EndpointObject;
        (*ep_obj).state
    };

    let mut msg = Message::new(label);
    msg.data_count = data_count;
    if data_count > 0
    {
        let buf = unsafe { (*tcb).ipc_buffer };
        unsafe { read_ipc_buf(buf, data_count, &mut msg.data) }?;
    }

    // SAFETY: ep_state is valid; scheduler lock not held.
    let result = unsafe { crate::ipc::endpoint::endpoint_call(ep_state, tcb, &msg) };

    match result
    {
        Ok(woken_server) =>
        {
            // A server was waiting; enqueue it and yield so it can run.
            // SAFETY: woken_server is a valid TCB.
            unsafe {
                let prio = (*woken_server).priority;
                crate::sched::scheduler_for(0).enqueue(woken_server, prio);
            }
        }
        Err(()) =>
        {
            // No server; caller is blocked on send queue. Yield to another thread.
        }
    }

    // Yield CPU — the current thread is now Blocked.
    // SAFETY: called from syscall handler.
    unsafe { crate::sched::schedule(); }

    // On resume (after reply), write reply results back to the caller.
    // SAFETY: tcb is still valid after resume.
    let reply_label = unsafe { (*tcb).ipc_msg.label };
    let reply_count = unsafe { (*tcb).ipc_msg.data_count };
    let reply_buf   = unsafe { (*tcb).ipc_buffer };

    if reply_count > 0
    {
        let data = unsafe { (*tcb).ipc_msg.data };
        unsafe { write_ipc_buf(reply_buf, reply_count, &data); }
    }

    tf.set_ipc_return( 0, reply_label);
    Ok(0) // primary return; set_ipc_return already wrote both values
}

/// SYS_IPC_RECV (2): receive the next message on an endpoint.
///
/// arg0 = endpoint cap index.
///
/// Blocks server until a caller sends. On return, the message label and data
/// words are available in return registers and the server's IPC buffer.
#[cfg(not(test))]
pub fn sys_ipc_recv(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    let ep_idx = tf.arg(0) as u32;

    let tcb = unsafe { current_tcb() };
    if tcb.is_null()
    {
        return Err(SyscallError::InvalidCapability);
    }
    let cspace_ptr = unsafe { (*tcb).cspace };
    let slot = unsafe { lookup_cap(cspace_ptr, ep_idx, CapTag::Endpoint, Rights::RECEIVE) }?;

    let ep_state = unsafe {
        let obj_ptr = slot.object.ok_or(SyscallError::InvalidCapability)?.as_ptr();
        let ep_obj = obj_ptr as *mut crate::cap::object::EndpointObject;
        (*ep_obj).state
    };

    // SAFETY: ep_state valid; scheduler lock not held.
    let result = unsafe { crate::ipc::endpoint::endpoint_recv(ep_state, tcb) };

    match result
    {
        Ok((_caller, msg)) =>
        {
            // A caller was waiting; deliver message immediately.
            let buf = unsafe { (*tcb).ipc_buffer };
            if msg.data_count > 0
            {
                unsafe { write_ipc_buf(buf, msg.data_count, &msg.data); }
            }
            tf.set_ipc_return( 0, msg.label);
            return Ok(0);
        }
        Err(()) =>
        {
            // No caller; server is now Blocked on recv queue. Yield.
        }
    }

    // SAFETY: called from syscall handler.
    unsafe { crate::sched::schedule(); }

    // On resume (caller arrived), deliver message from ipc_msg.
    let msg_label = unsafe { (*tcb).ipc_msg.label };
    let msg_count = unsafe { (*tcb).ipc_msg.data_count };
    let buf = unsafe { (*tcb).ipc_buffer };
    if msg_count > 0
    {
        let data = unsafe { (*tcb).ipc_msg.data };
        unsafe { write_ipc_buf(buf, msg_count, &data); }
    }
    tf.set_ipc_return( 0, msg_label);
    Ok(0)
}

/// SYS_IPC_REPLY (1): reply to a blocked caller.
///
/// arg0 = label, arg1 = data_count, arg2 = cap_slots (ignored), arg3 = flags.
#[cfg(not(test))]
pub fn sys_ipc_reply(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    let label      = tf.arg(0);
    let data_count = (tf.arg(1) as usize).min(MSG_DATA_WORDS_MAX);

    let tcb = unsafe { current_tcb() };
    if tcb.is_null()
    {
        return Err(SyscallError::InvalidCapability);
    }

    let mut msg = Message::new(label);
    msg.data_count = data_count;
    if data_count > 0
    {
        let buf = unsafe { (*tcb).ipc_buffer };
        unsafe { read_ipc_buf(buf, data_count, &mut msg.data) }?;
    }

    // SAFETY: ep_state valid; scheduler lock not held.
    let result = unsafe { crate::ipc::endpoint::endpoint_reply(tcb, &msg) };

    match result
    {
        Some(caller) =>
        {
            // Write reply into caller's IPC buffer, then re-enqueue caller.
            let caller_buf = unsafe { (*caller).ipc_buffer };
            if data_count > 0
            {
                unsafe { write_ipc_buf(caller_buf, data_count, &msg.data); }
            }
            // SAFETY: caller is a valid TCB.
            unsafe {
                let prio = (*caller).priority;
                crate::sched::scheduler_for(0).enqueue(caller, prio);
            }
            Ok(0)
        }
        None => Err(SyscallError::InvalidCapability),
    }
}

/// SYS_SIGNAL_SEND (3): OR bits into a signal object.
///
/// arg0 = signal cap index, arg1 = bits to send (must be non-zero).
#[cfg(not(test))]
pub fn sys_signal_send(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    let sig_idx = tf.arg(0) as u32;
    let bits    = tf.arg(1);

    if bits == 0
    {
        return Err(SyscallError::InvalidArgument);
    }

    let tcb = unsafe { current_tcb() };
    if tcb.is_null()
    {
        return Err(SyscallError::InvalidCapability);
    }
    let cspace_ptr = unsafe { (*tcb).cspace };
    let slot = unsafe { lookup_cap(cspace_ptr, sig_idx, CapTag::Signal, Rights::SIGNAL) }?;

    let sig_state = unsafe {
        let obj_ptr = slot.object.ok_or(SyscallError::InvalidCapability)?.as_ptr();
        let sig_obj = obj_ptr as *mut crate::cap::object::SignalObject;
        (*sig_obj).state
    };

    // SAFETY: sig_state valid; scheduler lock not held.
    let woken = unsafe { crate::ipc::signal::signal_send(sig_state, bits) };

    if let Some(waiter) = woken
    {
        // SAFETY: waiter is a valid TCB.
        unsafe {
            let prio = (*waiter).priority;
            crate::sched::scheduler_for(0).enqueue(waiter, prio);
        }
    }

    Ok(0)
}

/// SYS_SIGNAL_WAIT (4): block until a signal bit is set, then return the bits.
///
/// arg0 = signal cap index.
///
/// Returns the acquired bitmask in rax/a0.
#[cfg(not(test))]
pub fn sys_signal_wait(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    let sig_idx = tf.arg(0) as u32;

    let tcb = unsafe { current_tcb() };
    if tcb.is_null()
    {
        return Err(SyscallError::InvalidCapability);
    }
    let cspace_ptr = unsafe { (*tcb).cspace };
    let slot = unsafe { lookup_cap(cspace_ptr, sig_idx, CapTag::Signal, Rights::WAIT) }?;

    let sig_state = unsafe {
        let obj_ptr = slot.object.ok_or(SyscallError::InvalidCapability)?.as_ptr();
        let sig_obj = obj_ptr as *mut crate::cap::object::SignalObject;
        (*sig_obj).state
    };

    // SAFETY: sig_state valid; scheduler lock not held.
    let result = unsafe { crate::ipc::signal::signal_wait(sig_state, tcb) };

    match result
    {
        Ok(bits) =>
        {
            // Bits were already set; return immediately.
            Ok(bits)
        }
        Err(()) =>
        {
            // No bits; thread is Blocked. Yield CPU.
            // SAFETY: called from syscall handler.
            unsafe { crate::sched::schedule(); }

            // On resume, `signal_send` stored the delivered bits in wakeup_value.
            let bits = unsafe { (*tcb).wakeup_value };
            unsafe { (*tcb).wakeup_value = 0; }
            Ok(bits)
        }
    }
}
