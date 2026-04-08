// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/syscall/ipc.rs

//! IPC syscall handlers — W4.
//!
//! All handlers look up the target capability in the current thread's `CSpace`,
//! call the corresponding IPC kernel function, and enqueue/dequeue threads
//! via the scheduler as needed.
//!
//! Data words (up to `MSG_DATA_WORDS_MAX`) are read from / written to the
//! per-thread IPC buffer page registered via `SYS_IPC_BUFFER_SET`.
//!
//! Capability transfer (W4): up to `MSG_CAP_SLOTS_MAX` capabilities can be moved
//! atomically with each message. See `transfer_caps` for the protocol.

#[cfg(not(test))]
extern crate alloc;

#[cfg(not(test))]
use crate::arch::current::trap_frame::TrapFrame;
#[cfg(not(test))]
use syscall::SyscallError;

#[cfg(not(test))]
use super::{current_tcb, lookup_cap};
#[cfg(not(test))]
use crate::cap::slot::{CapTag, Rights};
#[cfg(not(test))]
use crate::cap::CSpace;
#[cfg(not(test))]
use crate::ipc::message::Message;
#[cfg(not(test))]
use syscall::{MSG_CAP_SLOTS_MAX, MSG_DATA_WORDS_MAX};

// ── IPC buffer helpers ────────────────────────────────────────────────────────

/// Read up to `count` data words from the thread's IPC buffer into `dst`.
///
/// Returns `Err(InvalidArgument)` if the buffer is not registered.
///
/// # Safety
/// `buf` must be a valid user-mode IPC buffer VA or 0. Brackets the access
/// with `user_access_begin`/`user_access_end` to satisfy SMAP (x86-64) /
/// SUM (RISC-V).
#[cfg(not(test))]
unsafe fn read_ipc_buf(
    buf: u64,
    count: usize,
    dst: &mut [u64; MSG_DATA_WORDS_MAX],
) -> Result<(), SyscallError>
{
    if buf == 0
    {
        return Err(SyscallError::InvalidArgument);
    }
    // cast_possible_truncation: Seraph targets 64-bit only; usize == u64 on all supported targets.
    #[allow(clippy::cast_possible_truncation)]
    let ptr = buf as *const u64;
    unsafe {
        crate::arch::current::cpu::user_access_begin();
        for (i, item) in dst.iter_mut().enumerate().take(count)
        {
            // SAFETY: buf is page-aligned and user-mapped; count <= MSG_DATA_WORDS_MAX.
            *item = core::ptr::read_volatile(ptr.add(i));
        }
        crate::arch::current::cpu::user_access_end();
    }
    Ok(())
}

/// Write up to `count` data words from `src` to the thread's IPC buffer.
///
/// Silently skips if buffer is not registered (caller may not want data).
///
/// # Safety
/// `buf` must be a valid user-mode IPC buffer VA or 0. Brackets the access
/// with `user_access_begin`/`user_access_end` to satisfy SMAP / SUM.
#[cfg(not(test))]
unsafe fn write_ipc_buf(buf: u64, count: usize, src: &[u64; MSG_DATA_WORDS_MAX])
{
    if buf == 0 || count == 0
    {
        return;
    }
    // cast_possible_truncation: Seraph targets 64-bit only; usize == u64 on all supported targets.
    #[allow(clippy::cast_possible_truncation)]
    let ptr = buf as *mut u64;
    unsafe {
        crate::arch::current::cpu::user_access_begin();
        for (i, item) in src.iter().enumerate().take(count)
        {
            // SAFETY: buf is page-aligned and user-mapped; count <= MSG_DATA_WORDS_MAX.
            core::ptr::write_volatile(ptr.add(i), *item);
        }
        crate::arch::current::cpu::user_access_end();
    }
}

/// Write cap transfer results to the receiver's IPC buffer.
///
/// Layout starting at word `MSG_DATA_WORDS_MAX`:
/// ```text
/// word[MSG_DATA_WORDS_MAX + 0] = cap_count as u64
/// word[MSG_DATA_WORDS_MAX + 1] = idx[0] as u64
/// word[MSG_DATA_WORDS_MAX + 2] = idx[1] as u64
/// ...
/// ```
///
/// Silently skips if `buf == 0`. Matches the layout in `shared/syscall`
/// `read_recv_caps`.
///
/// # Safety
/// `buf` must be a valid, mapped IPC buffer page VA, or 0.
#[cfg(not(test))]
unsafe fn write_cap_results(buf: u64, cap_count: usize, indices: &[u32; MSG_CAP_SLOTS_MAX])
{
    if buf == 0 || cap_count == 0
    {
        return;
    }
    // cast_possible_truncation: Seraph targets 64-bit only; usize == u64 on all supported targets.
    #[allow(clippy::cast_possible_truncation)]
    let ptr = buf as *mut u64;
    // SAFETY: IPC buffer is at least 4 KiB; MSG_DATA_WORDS_MAX + 1 + MSG_CAP_SLOTS_MAX
    // words = at most 11 words = 88 bytes, well within the page. Brackets the
    // access with user_access_begin/end to satisfy SMAP (x86-64) / SUM (RISC-V).
    unsafe {
        crate::arch::current::cpu::user_access_begin();
        core::ptr::write_volatile(ptr.add(MSG_DATA_WORDS_MAX), cap_count as u64);
        for (i, &idx) in indices.iter().enumerate().take(cap_count)
        {
            core::ptr::write_volatile(ptr.add(MSG_DATA_WORDS_MAX + 1 + i), u64::from(idx));
        }
        crate::arch::current::cpu::user_access_end();
    }
}

// ── Capability transfer ───────────────────────────────────────────────────────

/// Unpack `count` slot indices from a packed u64 (4 × u16).
///
/// Matches the encoding in `shared/syscall::pack_cap_slots`.
#[cfg(not(test))]
fn unpack_cap_slots(packed: u64, count: usize) -> [u32; MSG_CAP_SLOTS_MAX]
{
    let mut out = [0u32; MSG_CAP_SLOTS_MAX];
    for (i, item) in out.iter_mut().enumerate().take(count.min(MSG_CAP_SLOTS_MAX))
    {
        *item = ((packed >> (i * 16)) & 0xFFFF) as u32;
    }
    out
}

/// Move `src_slots[..cap_count]` from `src_cspace` to `dst_cspace`, writing
/// the new slot indices to `dst_ipc_buf`.
///
/// All-or-nothing: if any slot is null/invalid or the destination is full
/// (`pre_allocate` fails), returns an error and no caps are transferred.
///
/// On success, `dst_ipc_buf` receives the `cap_count` and new indices at the
/// fixed offset `MSG_DATA_WORDS_MAX` (see `write_cap_results`).
///
/// # Safety
/// `src_cspace` and `dst_cspace` must be valid live `CSpace` pointers.
/// `dst_ipc_buf` must be 0 or a valid mapped IPC buffer page VA.
///
/// # To add rollback on mid-transfer failure
/// Collect successfully-moved destination indices and call
/// `move_cap_between_cspaces` in reverse on failure. Currently the
/// pre-validation + pre-allocation pattern makes mid-transfer failure
/// unreachable in practice.
#[cfg(not(test))]
unsafe fn transfer_caps(
    src_cspace: *mut CSpace,
    src_slots: &[u32],
    dst_cspace: *mut CSpace,
    dst_ipc_buf: u64,
) -> Result<(), SyscallError>
{
    let cap_count = src_slots.len().min(MSG_CAP_SLOTS_MAX);
    if cap_count == 0
    {
        // Write zero cap_count to the IPC buffer so receivers see no caps.
        unsafe {
            write_cap_results(dst_ipc_buf, 0, &[0u32; MSG_CAP_SLOTS_MAX]);
        }
        return Ok(());
    }

    // Pre-validate: all source slots must be non-null.
    {
        let cs = unsafe { &*src_cspace };
        for &idx in &src_slots[..cap_count]
        {
            let slot = cs.slot(idx).ok_or(SyscallError::InvalidCapability)?;
            if slot.tag == CapTag::Null
            {
                return Err(SyscallError::InvalidCapability);
            }
        }
    }

    // Pre-allocate destination slots to avoid OOM mid-transfer.
    unsafe { (*dst_cspace).pre_allocate(cap_count) }.map_err(|_| SyscallError::OutOfMemory)?;

    // Acquire derivation lock for the batch move.
    crate::cap::DERIVATION_LOCK.write_lock();

    let mut dst_indices = [0u32; MSG_CAP_SLOTS_MAX];
    for (i, &src_idx) in src_slots[..cap_count].iter().enumerate()
    {
        // SAFETY: DERIVATION_LOCK held; CSpace pointers valid.
        dst_indices[i] =
            unsafe { crate::cap::move_cap_between_cspaces(src_cspace, src_idx, dst_cspace) }
                .unwrap_or_else(|_| {
                    // Pre-validation passed and pre-allocation succeeded; this branch
                    // is unreachable in correct operation. Panic in debug builds only.
                    debug_assert!(
                        false,
                        "transfer_caps: unexpected move failure after pre-validation"
                    );
                    0
                });
    }

    crate::cap::DERIVATION_LOCK.write_unlock();

    // Write results to receiver's IPC buffer.
    unsafe {
        write_cap_results(dst_ipc_buf, cap_count, &dst_indices);
    }

    Ok(())
}

// ── IPC syscall handlers ──────────────────────────────────────────────────────

/// `SYS_IPC_CALL` (0): synchronous call on an endpoint.
///
/// arg0 = endpoint cap index, arg1 = label, arg2 = `data_count`,
/// arg3 = `cap_count` (0-4), arg4 = packed cap slot indices (4 × u16).
///
/// If `cap_count` > 0, the endpoint cap must have `Rights::GRANT`. Capabilities
/// at the specified slots are moved from the caller's `CSpace` to the server's
/// `CSpace` atomically with the message.
///
/// Blocks caller until a server replies. On return, label and data words are
/// in the return registers and IPC buffer. Reply-direction cap indices are
/// written to the IPC buffer at word `MSG_DATA_WORDS_MAX` by the replier.
#[cfg(not(test))]
pub fn sys_ipc_call(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    // cast_possible_truncation: kernel runs on 64-bit only; value is bounded by kernel policy.
    #[allow(clippy::cast_possible_truncation)]
    let ep_idx = tf.arg(0) as u32;
    let label = tf.arg(1);
    // cast_possible_truncation: Seraph targets 64-bit only; usize == u64 on all supported targets.
    #[allow(clippy::cast_possible_truncation)]
    let data_count = (tf.arg(2) as usize).min(MSG_DATA_WORDS_MAX);
    // cast_possible_truncation: Seraph targets 64-bit only; usize == u64 on all supported targets.
    #[allow(clippy::cast_possible_truncation)]
    let cap_count = (tf.arg(3) as usize).min(MSG_CAP_SLOTS_MAX);
    let cap_packed = tf.arg(4);

    // SAFETY: current_tcb() valid from syscall context.
    let tcb = unsafe { current_tcb() };
    if tcb.is_null()
    {
        return Err(SyscallError::InvalidCapability);
    }

    // SAFETY: tcb is valid.
    let cspace_ptr = unsafe { (*tcb).cspace };

    // Determine required rights: SEND always; GRANT additionally when sending caps.
    let required_rights = if cap_count > 0
    {
        Rights::SEND | Rights::GRANT
    }
    else
    {
        Rights::SEND
    };

    let slot = unsafe { lookup_cap(cspace_ptr, ep_idx, CapTag::Endpoint, required_rights) }?;

    // Extract EndpointState pointer from the slot's object.
    // SAFETY: slot.object is a NonNull<KernelObjectHeader> at offset 0 of
    // EndpointObject; EndpointObject.state is valid.
    let ep_state = unsafe {
        let obj_ptr = slot.object.ok_or(SyscallError::InvalidCapability)?.as_ptr();
        // cast_ptr_alignment: kernel allocator guarantees object alignment; header is at the start of the allocation.
        #[allow(clippy::cast_ptr_alignment)]
        let ep_obj = obj_ptr.cast::<crate::cap::object::EndpointObject>();
        (*ep_obj).state
    };

    let mut msg = Message::new(label);
    msg.data_count = data_count;
    if data_count > 0
    {
        let buf = unsafe { (*tcb).ipc_buffer };
        unsafe { read_ipc_buf(buf, data_count, &mut msg.data) }?;
    }

    // Populate cap_slots in the message (indices in caller's CSpace).
    // The actual cap move happens in sys_ipc_recv after delivery.
    if cap_count > 0
    {
        let indices = unpack_cap_slots(cap_packed, cap_count);
        // Pre-validate source slots before blocking (all-or-nothing).
        {
            let cs = unsafe { &*cspace_ptr };
            for &idx in indices.iter().take(cap_count)
            {
                let slot = cs.slot(idx).ok_or(SyscallError::InvalidCapability)?;
                if slot.tag == CapTag::Null
                {
                    return Err(SyscallError::InvalidCapability);
                }
            }
        }
        msg.cap_slots = indices;
        msg.cap_count = cap_count;
    }

    // SAFETY: ep_state is valid; scheduler lock not held.
    let result = unsafe { crate::ipc::endpoint::endpoint_call(ep_state, tcb, &msg) };

    if let Ok(woken_server) = result
    {
        // A server was waiting; enqueue it and yield so it can run.
        // SAFETY: woken_server is a valid TCB.
        unsafe {
            let prio = (*woken_server).priority;
            crate::sched::scheduler_for(0).enqueue(woken_server, prio);
        }
    }
    // else: No server; caller is blocked on send queue. Yield to another thread.

    // Yield CPU — the current thread is now Blocked.
    // SAFETY: called from syscall handler.
    unsafe {
        crate::sched::schedule();
    }

    // On resume (after reply): write reply data to caller's IPC buffer.
    // Reply-direction caps were already written to our IPC buffer by sys_ipc_reply.
    // SAFETY: tcb is still valid after resume.
    let reply_label = unsafe { (*tcb).ipc_msg.label };
    let reply_count = unsafe { (*tcb).ipc_msg.data_count };
    let reply_buf = unsafe { (*tcb).ipc_buffer };

    if reply_count > 0
    {
        let data = unsafe { (*tcb).ipc_msg.data };
        unsafe {
            write_ipc_buf(reply_buf, reply_count, &data);
        }
    }

    tf.set_ipc_return(0, reply_label);
    Ok(0) // primary return; set_ipc_return already wrote both values
}

/// `SYS_IPC_RECV` (2): receive the next message on an endpoint.
///
/// arg0 = endpoint cap index.
///
/// Blocks server until a caller sends. On return, the message label and data
/// words are available in return registers and the server's IPC buffer.
/// If the message carried capabilities, their new slot indices in the server's
/// `CSpace` are written to the IPC buffer at word `MSG_DATA_WORDS_MAX`.
#[cfg(not(test))]
pub fn sys_ipc_recv(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    // cast_possible_truncation: kernel runs on 64-bit only; value is bounded by kernel policy.
    #[allow(clippy::cast_possible_truncation)]
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
        // cast_ptr_alignment: kernel allocator guarantees object alignment; header is at the start of the allocation.
        #[allow(clippy::cast_ptr_alignment)]
        let ep_obj = obj_ptr.cast::<crate::cap::object::EndpointObject>();
        (*ep_obj).state
    };

    // SAFETY: ep_state valid; scheduler lock not held.
    let result = unsafe { crate::ipc::endpoint::endpoint_recv(ep_state, tcb) };

    if let Ok((caller, msg)) = result
    {
        // A caller was waiting; deliver message immediately.
        let server_buf = unsafe { (*tcb).ipc_buffer };

        // Transfer caps from caller to server (if any).
        // On failure: all-or-nothing — return error. Caller stays blocked
        // on reply (TODO: for robustness, re-enqueue caller on send queue).
        if msg.cap_count > 0
        {
            let caller_cspace = unsafe { (*caller).cspace };
            let server_cspace = unsafe { (*tcb).cspace };
            unsafe {
                transfer_caps(
                    caller_cspace,
                    &msg.cap_slots[..msg.cap_count],
                    server_cspace,
                    server_buf,
                )
            }?;
        }

        if msg.data_count > 0
        {
            unsafe {
                write_ipc_buf(server_buf, msg.data_count, &msg.data);
            }
        }
        tf.set_ipc_return(0, msg.label);
        return Ok(0);
    }
    // else: No caller; server is now Blocked on recv queue. Yield.

    // SAFETY: called from syscall handler.
    unsafe {
        crate::sched::schedule();
    }

    // On resume (caller arrived), deliver message from ipc_msg.
    // SAFETY: tcb valid; reply_tcb points to the caller that woke us.
    let msg = unsafe { (*tcb).ipc_msg };
    let server_buf = unsafe { (*tcb).ipc_buffer };

    if msg.cap_count > 0
    {
        let caller = unsafe { (*tcb).reply_tcb };
        let caller_cspace = unsafe { (*caller).cspace };
        let server_cspace = unsafe { (*tcb).cspace };
        // On failure: deliver message data without caps (cap_count=0 in buf).
        // The caller stays blocked awaiting a reply; the server receives an
        // incomplete message but is not crashed.
        let _ = unsafe {
            transfer_caps(
                caller_cspace,
                &msg.cap_slots[..msg.cap_count],
                server_cspace,
                server_buf,
            )
        };
    }

    if msg.data_count > 0
    {
        unsafe {
            write_ipc_buf(server_buf, msg.data_count, &msg.data);
        }
    }
    tf.set_ipc_return(0, msg.label);
    Ok(0)
}

/// `SYS_IPC_REPLY` (1): reply to a blocked caller.
///
/// arg0 = label, arg1 = `data_count`,
/// arg2 = `cap_count` (0-4), arg3 = packed cap slot indices (4 × u16).
///
/// Capabilities are moved from the server's `CSpace` to the caller's `CSpace`
/// atomically with the reply. No GRANT right check: the server is trusted
/// by virtue of holding RECEIVE on the endpoint.
#[cfg(not(test))]
pub fn sys_ipc_reply(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    let label = tf.arg(0);
    // cast_possible_truncation: Seraph targets 64-bit only; usize == u64 on all supported targets.
    #[allow(clippy::cast_possible_truncation)]
    let data_count = (tf.arg(1) as usize).min(MSG_DATA_WORDS_MAX);
    // cast_possible_truncation: Seraph targets 64-bit only; usize == u64 on all supported targets.
    #[allow(clippy::cast_possible_truncation)]
    let cap_count = (tf.arg(2) as usize).min(MSG_CAP_SLOTS_MAX);
    let cap_packed = tf.arg(3);

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

    // Populate cap_slots (indices in server's CSpace). Pre-validate before reply.
    if cap_count > 0
    {
        let server_cspace = unsafe { (*tcb).cspace };
        let indices = unpack_cap_slots(cap_packed, cap_count);
        {
            let cs = unsafe { &*server_cspace };
            for &idx in indices.iter().take(cap_count)
            {
                let slot = cs.slot(idx).ok_or(SyscallError::InvalidCapability)?;
                if slot.tag == CapTag::Null
                {
                    return Err(SyscallError::InvalidCapability);
                }
            }
        }
        msg.cap_slots = indices;
        msg.cap_count = cap_count;
    }

    // SAFETY: ep_state valid; scheduler lock not held.
    let result = unsafe { crate::ipc::endpoint::endpoint_reply(tcb, &msg) };

    match result
    {
        Some(caller) =>
        {
            let caller_buf = unsafe { (*caller).ipc_buffer };

            // Transfer caps from server to caller (if any).
            if cap_count > 0
            {
                let server_cspace = unsafe { (*tcb).cspace };
                let caller_cspace = unsafe { (*caller).cspace };
                unsafe {
                    transfer_caps(
                        server_cspace,
                        &msg.cap_slots[..cap_count],
                        caller_cspace,
                        caller_buf,
                    )
                }?;
                // On failure: ? propagates the error to the server. The caller
                // stays blocked. The server can retry.
            }
            else
            {
                // Write zero cap_count so caller can read consistently.
                unsafe {
                    write_cap_results(caller_buf, 0, &[0u32; MSG_CAP_SLOTS_MAX]);
                }
            }

            // Write reply data to caller's IPC buffer.
            if data_count > 0
            {
                unsafe {
                    write_ipc_buf(caller_buf, data_count, &msg.data);
                }
            }

            // Re-enqueue caller (it is now Ready).
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

/// `SYS_SIGNAL_SEND` (3): OR bits into a signal object.
///
/// arg0 = signal cap index, arg1 = bits to send (must be non-zero).
#[cfg(not(test))]
pub fn sys_signal_send(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    // cast_possible_truncation: kernel runs on 64-bit only; value is bounded by kernel policy.
    #[allow(clippy::cast_possible_truncation)]
    let sig_idx = tf.arg(0) as u32;
    let bits = tf.arg(1);

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
        // cast_ptr_alignment: kernel allocator guarantees object alignment; header is at the start of the allocation.
        #[allow(clippy::cast_ptr_alignment)]
        let sig_obj = obj_ptr.cast::<crate::cap::object::SignalObject>();
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

/// `SYS_SIGNAL_WAIT` (4): block until a signal bit is set, then return the bits.
///
/// arg0 = signal cap index.
///
/// Returns the acquired bitmask in rax/a0.
#[cfg(not(test))]
pub fn sys_signal_wait(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    // cast_possible_truncation: kernel runs on 64-bit only; value is bounded by kernel policy.
    #[allow(clippy::cast_possible_truncation)]
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
        // cast_ptr_alignment: kernel allocator guarantees object alignment; header is at the start of the allocation.
        #[allow(clippy::cast_ptr_alignment)]
        let sig_obj = obj_ptr.cast::<crate::cap::object::SignalObject>();
        (*sig_obj).state
    };

    // SAFETY: sig_state valid; scheduler lock not held.
    let result = unsafe { crate::ipc::signal::signal_wait(sig_state, tcb) };

    if let Ok(bits) = result
    {
        // Bits were already set; return immediately.
        return Ok(bits);
    }

    // No bits; thread is Blocked. Yield CPU.
    // SAFETY: called from syscall handler.
    unsafe {
        crate::sched::schedule();
    }

    // On resume, `signal_send` stored the delivered bits in wakeup_value.
    let bits = unsafe { (*tcb).wakeup_value };
    unsafe {
        (*tcb).wakeup_value = 0;
    }
    Ok(bits)
}

// ── Event Queue handlers ──────────────────────────────────────────────────────

/// `SYS_EVENT_POST` (5): append a payload word to an event queue (non-blocking).
///
/// arg0 = event queue cap index (must have POST right).
/// arg1 = payload word to enqueue.
///
/// Returns `SyscallError::QueueFull` if the queue is at capacity.
/// Returns `SyscallError::InvalidArgument` if bits == 0 is not a constraint
/// (any u64 payload including 0 is valid for event queues).
#[cfg(not(test))]
pub fn sys_event_post(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    // cast_possible_truncation: kernel runs on 64-bit only; value is bounded by kernel policy.
    #[allow(clippy::cast_possible_truncation)]
    let eq_idx = tf.arg(0) as u32;
    let payload = tf.arg(1);

    let tcb = unsafe { current_tcb() };
    if tcb.is_null()
    {
        return Err(SyscallError::InvalidCapability);
    }
    let cspace_ptr = unsafe { (*tcb).cspace };
    let slot = unsafe { lookup_cap(cspace_ptr, eq_idx, CapTag::EventQueue, Rights::POST) }?;

    let eq_state = unsafe {
        let obj_ptr = slot.object.ok_or(SyscallError::InvalidCapability)?.as_ptr();
        // cast_ptr_alignment: kernel allocator guarantees object alignment; header is at the start of the allocation.
        #[allow(clippy::cast_ptr_alignment)]
        let eq_obj = obj_ptr.cast::<crate::cap::object::EventQueueObject>();
        (*eq_obj).state
    };

    // SAFETY: eq_state valid; scheduler lock not held.
    let result = unsafe { crate::ipc::event_queue::event_queue_post(eq_state, payload) };

    match result
    {
        Ok(Some(woken)) =>
        {
            // A waiter was woken; enqueue it.
            // SAFETY: woken is a valid TCB.
            unsafe {
                let prio = (*woken).priority;
                crate::sched::scheduler_for(0).enqueue(woken, prio);
            }
            Ok(0)
        }
        Ok(None) => Ok(0),
        Err(()) => Err(SyscallError::QueueFull),
    }
}

/// `SYS_EVENT_RECV` (6): dequeue the next entry from an event queue.
///
/// arg0 = event queue cap index (must have RECV right).
///
/// Blocks if the queue is empty. On return, the payload is in the secondary
/// return register (rdx on x86-64, a1 on RISC-V).
#[cfg(not(test))]
pub fn sys_event_recv(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    // cast_possible_truncation: kernel runs on 64-bit only; value is bounded by kernel policy.
    #[allow(clippy::cast_possible_truncation)]
    let eq_idx = tf.arg(0) as u32;

    let tcb = unsafe { current_tcb() };
    if tcb.is_null()
    {
        return Err(SyscallError::InvalidCapability);
    }
    let cspace_ptr = unsafe { (*tcb).cspace };
    let slot = unsafe { lookup_cap(cspace_ptr, eq_idx, CapTag::EventQueue, Rights::RECV) }?;

    let eq_state = unsafe {
        let obj_ptr = slot.object.ok_or(SyscallError::InvalidCapability)?.as_ptr();
        // cast_ptr_alignment: kernel allocator guarantees object alignment; header is at the start of the allocation.
        #[allow(clippy::cast_ptr_alignment)]
        let eq_obj = obj_ptr.cast::<crate::cap::object::EventQueueObject>();
        (*eq_obj).state
    };

    // SAFETY: eq_state valid; scheduler lock not held.
    let result = unsafe { crate::ipc::event_queue::event_queue_recv(eq_state, tcb) };

    if let Ok(payload) = result
    {
        // Entry available immediately; deliver payload in secondary register.
        tf.set_ipc_return(0, payload);
        return Ok(0);
    }
    // else: Queue empty; thread is Blocked. Yield CPU.

    // SAFETY: called from syscall handler.
    unsafe {
        crate::sched::schedule();
    }

    // On resume, event_queue_post stored the payload in wakeup_value.
    let payload = unsafe { (*tcb).wakeup_value };
    unsafe {
        (*tcb).wakeup_value = 0;
    }
    tf.set_ipc_return(0, payload);
    Ok(0)
}

// ── Wait Set handlers ─────────────────────────────────────────────────────────

/// `SYS_WAIT_SET_ADD` (26): register a source in a wait set.
///
/// arg0 = wait set cap index (must have MODIFY right).
/// arg1 = source cap index (Endpoint with RECEIVE, Signal with WAIT, or
///        `EventQueue` with RECV).
/// arg2 = caller-chosen opaque u64 token (returned by `SYS_WAIT_SET_WAIT`
///        when this source fires).
///
/// Returns `SyscallError::InvalidArgument` if the wait set is full
/// or the source is already registered in a wait set.
// too_many_lines: this function performs a single logical operation (cap resolution + wait set
// registration) that requires dispatching over three source types; splitting it would
// obscure the all-or-nothing atomicity contract.
#[cfg(not(test))]
#[allow(clippy::too_many_lines)]
pub fn sys_wait_set_add(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    use crate::ipc::wait_set::WaitSetSourceTag;

    // cast_possible_truncation: kernel runs on 64-bit only; value is bounded by kernel policy.
    #[allow(clippy::cast_possible_truncation)]
    let ws_idx = tf.arg(0) as u32;
    // cast_possible_truncation: kernel runs on 64-bit only; value is bounded by kernel policy.
    #[allow(clippy::cast_possible_truncation)]
    let src_idx = tf.arg(1) as u32;
    let token = tf.arg(2);

    let tcb = unsafe { current_tcb() };
    if tcb.is_null()
    {
        return Err(SyscallError::InvalidCapability);
    }
    let cspace_ptr = unsafe { (*tcb).cspace };

    // Look up wait set cap.
    let ws_slot = unsafe { lookup_cap(cspace_ptr, ws_idx, CapTag::WaitSet, Rights::MODIFY) }?;
    let ws_state = unsafe {
        let obj_ptr = ws_slot
            .object
            .ok_or(SyscallError::InvalidCapability)?
            .as_ptr();
        // cast_ptr_alignment: kernel allocator guarantees object alignment; header is at the start of the allocation.
        #[allow(clippy::cast_ptr_alignment)]
        let ws_obj = obj_ptr.cast::<crate::cap::object::WaitSetObject>();
        (*ws_obj).state
    };

    // Look up source cap — accept Endpoint, Signal, or EventQueue.
    // We must accept any of the three tags, so we read the slot directly
    // instead of calling lookup_cap (which checks a single expected tag).
    let (source_ptr, source_tag) = unsafe {
        let cs = &*cspace_ptr;
        let src_slot = cs.slot(src_idx).ok_or(SyscallError::InvalidCapability)?;
        match src_slot.tag
        {
            CapTag::Endpoint =>
            {
                if !src_slot.rights.contains(Rights::RECEIVE)
                {
                    return Err(SyscallError::InsufficientRights);
                }
                let obj_ptr = src_slot
                    .object
                    .ok_or(SyscallError::InvalidCapability)?
                    .as_ptr();
                // cast_ptr_alignment: kernel allocator guarantees object alignment; header is at the start of the allocation.
                #[allow(clippy::cast_ptr_alignment)]
                let ep_obj = obj_ptr.cast::<crate::cap::object::EndpointObject>();
                let ep_state = (*ep_obj).state.cast::<u8>();
                (ep_state, WaitSetSourceTag::Endpoint)
            }
            CapTag::Signal =>
            {
                if !src_slot.rights.contains(Rights::WAIT)
                {
                    return Err(SyscallError::InsufficientRights);
                }
                let obj_ptr = src_slot
                    .object
                    .ok_or(SyscallError::InvalidCapability)?
                    .as_ptr();
                // cast_ptr_alignment: kernel allocator guarantees object alignment; header is at the start of the allocation.
                #[allow(clippy::cast_ptr_alignment)]
                let sig_obj = obj_ptr.cast::<crate::cap::object::SignalObject>();
                let sig_state = (*sig_obj).state.cast::<u8>();
                (sig_state, WaitSetSourceTag::Signal)
            }
            CapTag::EventQueue =>
            {
                if !src_slot.rights.contains(Rights::RECV)
                {
                    return Err(SyscallError::InsufficientRights);
                }
                let obj_ptr = src_slot
                    .object
                    .ok_or(SyscallError::InvalidCapability)?
                    .as_ptr();
                // cast_ptr_alignment: kernel allocator guarantees object alignment; header is at the start of the allocation.
                #[allow(clippy::cast_ptr_alignment)]
                let eq_obj = obj_ptr.cast::<crate::cap::object::EventQueueObject>();
                let eq_state = (*eq_obj).state.cast::<u8>();
                (eq_state, WaitSetSourceTag::EventQueue)
            }
            _ => return Err(SyscallError::InvalidCapability),
        }
    };

    // Check the source is not already in a wait set (single wait set invariant).
    // The `wait_set` field is at the same offset for all three source types.
    // We check it by reading the field directly based on the tag.
    let already_registered = unsafe {
        match source_tag
        {
            WaitSetSourceTag::Endpoint =>
            {
                // cast_ptr_alignment: kernel allocator guarantees object alignment; header is at the start of the allocation.
                #[allow(clippy::cast_ptr_alignment)]
                let ep = source_ptr.cast::<crate::ipc::endpoint::EndpointState>();
                !(*ep).wait_set.is_null()
            }
            WaitSetSourceTag::Signal =>
            {
                // cast_ptr_alignment: kernel allocator guarantees object alignment; header is at the start of the allocation.
                #[allow(clippy::cast_ptr_alignment)]
                let sig = source_ptr.cast::<crate::ipc::signal::SignalState>();
                !(*sig).wait_set.is_null()
            }
            WaitSetSourceTag::EventQueue =>
            {
                // cast_ptr_alignment: kernel allocator guarantees object alignment; header is at the start of the allocation.
                #[allow(clippy::cast_ptr_alignment)]
                let eq = source_ptr.cast::<crate::ipc::event_queue::EventQueueState>();
                !(*eq).wait_set.is_null()
            }
        }
    };
    if already_registered
    {
        return Err(SyscallError::InvalidArgument);
    }

    // Register in the wait set.
    // SAFETY: ws_state, source_ptr valid; scheduler lock not held.
    let member_idx =
        unsafe { crate::ipc::wait_set::waitset_add(ws_state, source_ptr, source_tag, token) }
            .map_err(|()| SyscallError::InvalidArgument)?;

    // Write back-pointer on the source.
    unsafe {
        match source_tag
        {
            WaitSetSourceTag::Endpoint =>
            {
                // cast_ptr_alignment: kernel allocator guarantees object alignment; header is at the start of the allocation.
                #[allow(clippy::cast_ptr_alignment)]
                let ep = source_ptr.cast::<crate::ipc::endpoint::EndpointState>();
                (*ep).wait_set = ws_state.cast::<u8>();
                (*ep).wait_set_member_idx = member_idx;
            }
            WaitSetSourceTag::Signal =>
            {
                // cast_ptr_alignment: kernel allocator guarantees object alignment; header is at the start of the allocation.
                #[allow(clippy::cast_ptr_alignment)]
                let sig = source_ptr.cast::<crate::ipc::signal::SignalState>();
                (*sig).wait_set = ws_state.cast::<u8>();
                (*sig).wait_set_member_idx = member_idx;
            }
            WaitSetSourceTag::EventQueue =>
            {
                // cast_ptr_alignment: kernel allocator guarantees object alignment; header is at the start of the allocation.
                #[allow(clippy::cast_ptr_alignment)]
                let eq = source_ptr.cast::<crate::ipc::event_queue::EventQueueState>();
                (*eq).wait_set = ws_state.cast::<u8>();
                (*eq).wait_set_member_idx = member_idx;
            }
        }
    }

    Ok(0)
}

/// `SYS_WAIT_SET_REMOVE` (27): unregister a source from a wait set.
///
/// arg0 = wait set cap index (must have MODIFY right).
/// arg1 = source cap index (same cap used to add).
///
/// Clears the back-pointer on the source. Stale entries for the removed
/// member are silently skipped by subsequent `SYS_WAIT_SET_WAIT` calls.
#[cfg(not(test))]
pub fn sys_wait_set_remove(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    use crate::ipc::wait_set::WaitSetSourceTag;

    // cast_possible_truncation: kernel runs on 64-bit only; value is bounded by kernel policy.
    #[allow(clippy::cast_possible_truncation)]
    let ws_idx = tf.arg(0) as u32;
    // cast_possible_truncation: kernel runs on 64-bit only; value is bounded by kernel policy.
    #[allow(clippy::cast_possible_truncation)]
    let src_idx = tf.arg(1) as u32;

    let tcb = unsafe { current_tcb() };
    if tcb.is_null()
    {
        return Err(SyscallError::InvalidCapability);
    }
    let cspace_ptr = unsafe { (*tcb).cspace };

    // Look up wait set.
    let ws_slot = unsafe { lookup_cap(cspace_ptr, ws_idx, CapTag::WaitSet, Rights::MODIFY) }?;
    let ws_state = unsafe {
        let obj_ptr = ws_slot
            .object
            .ok_or(SyscallError::InvalidCapability)?
            .as_ptr();
        // cast_ptr_alignment: kernel allocator guarantees object alignment; header is at the start of the allocation.
        #[allow(clippy::cast_ptr_alignment)]
        let ws_obj = obj_ptr.cast::<crate::cap::object::WaitSetObject>();
        (*ws_obj).state
    };

    // Look up source to get its raw state pointer.
    let (source_ptr, source_tag) = unsafe {
        let cs = &*cspace_ptr;
        let src_slot = cs.slot(src_idx).ok_or(SyscallError::InvalidCapability)?;
        match src_slot.tag
        {
            CapTag::Endpoint =>
            {
                let obj_ptr = src_slot
                    .object
                    .ok_or(SyscallError::InvalidCapability)?
                    .as_ptr();
                // cast_ptr_alignment: kernel allocator guarantees object alignment; header is at the start of the allocation.
                #[allow(clippy::cast_ptr_alignment)]
                let ep_obj = obj_ptr.cast::<crate::cap::object::EndpointObject>();
                ((*ep_obj).state.cast::<u8>(), WaitSetSourceTag::Endpoint)
            }
            CapTag::Signal =>
            {
                let obj_ptr = src_slot
                    .object
                    .ok_or(SyscallError::InvalidCapability)?
                    .as_ptr();
                // cast_ptr_alignment: kernel allocator guarantees object alignment; header is at the start of the allocation.
                #[allow(clippy::cast_ptr_alignment)]
                let sig_obj = obj_ptr.cast::<crate::cap::object::SignalObject>();
                ((*sig_obj).state.cast::<u8>(), WaitSetSourceTag::Signal)
            }
            CapTag::EventQueue =>
            {
                let obj_ptr = src_slot
                    .object
                    .ok_or(SyscallError::InvalidCapability)?
                    .as_ptr();
                // cast_ptr_alignment: kernel allocator guarantees object alignment; header is at the start of the allocation.
                #[allow(clippy::cast_ptr_alignment)]
                let eq_obj = obj_ptr.cast::<crate::cap::object::EventQueueObject>();
                ((*eq_obj).state.cast::<u8>(), WaitSetSourceTag::EventQueue)
            }
            _ => return Err(SyscallError::InvalidCapability),
        }
    };

    // Remove from wait set.
    // SAFETY: ws_state, source_ptr valid.
    unsafe { crate::ipc::wait_set::waitset_remove(ws_state, source_ptr) }
        .map_err(|()| SyscallError::InvalidCapability)?;

    // Clear source back-pointer.
    unsafe {
        match source_tag
        {
            WaitSetSourceTag::Endpoint =>
            {
                // cast_ptr_alignment: kernel allocator guarantees object alignment; header is at the start of the allocation.
                #[allow(clippy::cast_ptr_alignment)]
                let ep = source_ptr.cast::<crate::ipc::endpoint::EndpointState>();
                (*ep).wait_set = core::ptr::null_mut();
                (*ep).wait_set_member_idx = 0;
            }
            WaitSetSourceTag::Signal =>
            {
                // cast_ptr_alignment: kernel allocator guarantees object alignment; header is at the start of the allocation.
                #[allow(clippy::cast_ptr_alignment)]
                let sig = source_ptr.cast::<crate::ipc::signal::SignalState>();
                (*sig).wait_set = core::ptr::null_mut();
                (*sig).wait_set_member_idx = 0;
            }
            WaitSetSourceTag::EventQueue =>
            {
                // cast_ptr_alignment: kernel allocator guarantees object alignment; header is at the start of the allocation.
                #[allow(clippy::cast_ptr_alignment)]
                let eq = source_ptr.cast::<crate::ipc::event_queue::EventQueueState>();
                (*eq).wait_set = core::ptr::null_mut();
                (*eq).wait_set_member_idx = 0;
            }
        }
    }

    Ok(0)
}

/// `SYS_WAIT_SET_WAIT` (28): block until any registered source becomes ready.
///
/// arg0 = wait set cap index (must have WAIT right).
///
/// If a source is already ready (from a prior notification), returns immediately.
/// Otherwise blocks until any member fires. The opaque token chosen at
/// `SYS_WAIT_SET_ADD` time is returned in the secondary return register
/// (rdx on x86-64, a1 on RISC-V). The caller then reads from the identified
/// source normally.
#[cfg(not(test))]
pub fn sys_wait_set_wait(tf: &mut TrapFrame) -> Result<u64, SyscallError>
{
    // cast_possible_truncation: kernel runs on 64-bit only; value is bounded by kernel policy.
    #[allow(clippy::cast_possible_truncation)]
    let ws_idx = tf.arg(0) as u32;

    let tcb = unsafe { current_tcb() };
    if tcb.is_null()
    {
        return Err(SyscallError::InvalidCapability);
    }
    let cspace_ptr = unsafe { (*tcb).cspace };
    let ws_slot = unsafe { lookup_cap(cspace_ptr, ws_idx, CapTag::WaitSet, Rights::WAIT) }?;

    let ws_state = unsafe {
        let obj_ptr = ws_slot
            .object
            .ok_or(SyscallError::InvalidCapability)?
            .as_ptr();
        // cast_ptr_alignment: kernel allocator guarantees object alignment; header is at the start of the allocation.
        #[allow(clippy::cast_ptr_alignment)]
        let ws_obj = obj_ptr.cast::<crate::cap::object::WaitSetObject>();
        (*ws_obj).state
    };

    // SAFETY: ws_state valid; scheduler lock not held.
    let result = unsafe { crate::ipc::wait_set::waitset_wait(ws_state, tcb) };

    if let Ok(token) = result
    {
        // A source was already ready; return its token.
        tf.set_ipc_return(0, token);
        return Ok(0);
    }
    // else: No source ready; thread is Blocked. Yield CPU.

    // SAFETY: called from syscall handler.
    unsafe {
        crate::sched::schedule();
    }

    // On resume, waitset_notify stored the token in wakeup_value.
    let token = unsafe { (*tcb).wakeup_value };
    unsafe {
        (*tcb).wakeup_value = 0;
    }
    tf.set_ipc_return(0, token);
    Ok(0)
}
