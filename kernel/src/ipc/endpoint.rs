// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/ipc/endpoint.rs

//! Endpoint IPC — synchronous call / receive / reply.
//!
//! An endpoint has two intrusive FIFO queues:
//! - `send_queue`: callers blocked waiting for a server to `recv`.
//! - `recv_queue`: servers blocked waiting for a caller to `call`.
//!
//! ## Protocol
//! 1. Caller: `call(ep, msg)` — if a server is waiting → transfer message,
//!    mint reply capability, wake server, block caller on reply.
//!    Otherwise → enqueue caller on send_queue.
//! 2. Server: `recv(ep)` — if a caller is waiting → dequeue, transfer message,
//!    mint reply cap, return to server. Otherwise → block on recv_queue.
//! 3. Server: `reply(reply_cap, msg)` — transfer reply, wake caller, consume cap.
//!
//! ## Reply capability
//! Phase 9 uses a simple approach: the "reply cap" is stored directly in the
//! caller's TCB (`reply_tcb` field). The server's `reply_cap_slot` points at the
//! caller's TCB. Full derivation-tree reply caps are deferred to a future phase.
//!
//! ## Thread safety
//! All operations must be called with the relevant scheduler lock held.

use super::message::Message;
use crate::sched::thread::{IpcThreadState, ThreadControlBlock, ThreadState};

// ── EndpointState ─────────────────────────────────────────────────────────────

/// Kernel state backing an Endpoint capability.
///
/// The send/recv queues are intrusive singly-linked lists through `ipc_wait_next`
/// in each TCB. Both queues have FIFO ordering.
pub struct EndpointState
{
    /// Head of the blocked-senders queue (callers waiting for a receiver).
    pub send_head: *mut ThreadControlBlock,
    /// Tail of the blocked-senders queue.
    pub send_tail: *mut ThreadControlBlock,
    /// Head of the blocked-receivers queue (servers waiting for a caller).
    pub recv_head: *mut ThreadControlBlock,
    /// Tail of the blocked-receivers queue.
    pub recv_tail: *mut ThreadControlBlock,
    /// Opaque pointer to the `WaitSetState` this endpoint is registered with,
    /// or null if not in any wait set. Type-erased to avoid a circular import.
    /// Cast to `*mut WaitSetState` only inside wait_set.rs.
    pub wait_set: *mut u8,
    /// Index of this endpoint's entry in `WaitSetState::members`.
    pub wait_set_member_idx: u8,
}

// SAFETY: EndpointState is accessed only under the relevant scheduler lock.
unsafe impl Send for EndpointState {}
unsafe impl Sync for EndpointState {}

impl EndpointState
{
    /// Create a new, empty endpoint with no waiting threads.
    pub fn new() -> Self
    {
        Self {
            send_head: core::ptr::null_mut(),
            send_tail: core::ptr::null_mut(),
            recv_head: core::ptr::null_mut(),
            recv_tail: core::ptr::null_mut(),
            wait_set: core::ptr::null_mut(),
            wait_set_member_idx: 0,
        }
    }
}

// ── Queue helpers ─────────────────────────────────────────────────────────────

/// Append `tcb` to the tail of a FIFO queue (head, tail pointers).
///
/// # Safety
/// The TCB must not already be on any queue.
unsafe fn enqueue(head: &mut *mut ThreadControlBlock, tail: &mut *mut ThreadControlBlock, tcb: *mut ThreadControlBlock)
{
    // SAFETY: tcb is a valid TCB.
    unsafe { (*tcb).ipc_wait_next = None; }
    if tail.is_null()
    {
        *head = tcb;
        *tail = tcb;
    }
    else
    {
        // SAFETY: *tail is a valid TCB.
        unsafe { (**tail).ipc_wait_next = Some(tcb); }
        *tail = tcb;
    }
}

/// Remove and return the head of the queue, or null if empty.
///
/// # Safety
/// Head/tail pointers must be consistent.
unsafe fn dequeue(head: &mut *mut ThreadControlBlock, tail: &mut *mut ThreadControlBlock) -> *mut ThreadControlBlock
{
    if head.is_null()
    {
        return core::ptr::null_mut();
    }
    let tcb = *head;
    // SAFETY: tcb is a valid TCB.
    let next = unsafe { (*tcb).ipc_wait_next };
    *head = next.unwrap_or(core::ptr::null_mut());
    if head.is_null()
    {
        *tail = core::ptr::null_mut();
    }
    // SAFETY: tcb is valid.
    unsafe { (*tcb).ipc_wait_next = None; }
    tcb
}

// ── Endpoint operations ───────────────────────────────────────────────────────

/// Attempt an IPC call on `ep` from `caller` with `msg`.
///
/// Returns `Ok(woken_server)` if a receiver was waiting and was woken (caller
/// is now blocked on reply). Returns `Err(())` if no receiver was available
/// (caller is now blocked on the send queue).
///
/// # Safety
/// Must be called with the scheduler lock held.
#[cfg(not(test))]
pub unsafe fn endpoint_call(
    ep: *mut EndpointState,
    caller: *mut ThreadControlBlock,
    msg: &Message,
) -> Result<*mut ThreadControlBlock, ()>
{
    // SAFETY: caller guarantees ep is valid and lock is held.
    let ep = unsafe { &mut *ep };

    // Is a server waiting?
    let server = unsafe { dequeue(&mut ep.recv_head, &mut ep.recv_tail) };
    if !server.is_null()
    {
        // Transfer message to server.
        // SAFETY: server is a valid TCB.
        unsafe {
            (*server).ipc_msg = *msg;
            // Store caller as the reply target in the server's TCB.
            (*server).reply_tcb = caller;
            (*server).state = ThreadState::Ready;
            (*server).ipc_state = IpcThreadState::None;
            (*server).blocked_on_object = core::ptr::null_mut();
        }
        // Block caller on reply; record the server as the blocking object so
        // SYS_THREAD_STOP can cancel by clearing server.reply_tcb.
        // SAFETY: caller is a valid TCB.
        unsafe {
            (*caller).state = ThreadState::Blocked;
            (*caller).ipc_state = IpcThreadState::BlockedOnReply;
            (*caller).blocked_on_object = server as *mut u8;
        }
        return Ok(server);
    }

    // No server available — block caller on send queue.
    // SAFETY: caller is valid.
    let was_empty = ep.send_head.is_null();
    unsafe {
        (*caller).ipc_msg = *msg;
        (*caller).state = ThreadState::Blocked;
        (*caller).ipc_state = IpcThreadState::BlockedOnSend;
        (*caller).blocked_on_object = ep as *mut EndpointState as *mut u8;
        enqueue(&mut ep.send_head, &mut ep.send_tail, caller);
    }
    // Notify a registered wait set on the transition from empty → non-empty.
    // Only fire on the first pending sender to avoid duplicate wakes.
    if was_empty && !ep.wait_set.is_null()
    {
        // SAFETY: wait_set is a valid *mut WaitSetState registered by
        // sys_wait_set_add and cleared on removal or wait_set_drop.
        unsafe { crate::ipc::wait_set::waitset_notify(ep.wait_set, ep.wait_set_member_idx) };
    }
    Err(())
}

/// Attempt to receive on `ep` as `server`.
///
/// Returns `Ok(caller, msg)` if a sender was waiting (server continues running;
/// sender remains blocked on reply). Returns `Err(())` if no sender was available
/// (server is now blocked on the recv queue).
///
/// # Safety
/// Must be called with the scheduler lock held.
#[cfg(not(test))]
pub unsafe fn endpoint_recv(
    ep: *mut EndpointState,
    server: *mut ThreadControlBlock,
) -> Result<(*mut ThreadControlBlock, Message), ()>
{
    let ep = unsafe { &mut *ep };

    let caller = unsafe { dequeue(&mut ep.send_head, &mut ep.send_tail) };
    if !caller.is_null()
    {
        // Dequeue the pending call and deliver to server.
        let msg = unsafe { (*caller).ipc_msg };
        // Record who the server should reply to.
        unsafe { (*server).reply_tcb = caller; }
        // Update caller's blocking state: it is now waiting for this server's
        // reply, not in the send queue. SYS_THREAD_STOP will use blocked_on_object
        // (the server TCB) to cancel the reply if needed.
        // SAFETY: caller is a valid TCB.
        unsafe {
            (*caller).ipc_state = IpcThreadState::BlockedOnReply;
            (*caller).blocked_on_object = server as *mut u8;
        }
        return Ok((caller, msg));
    }

    // No sender — block server on recv queue.
    unsafe {
        (*server).state = ThreadState::Blocked;
        (*server).ipc_state = IpcThreadState::BlockedOnRecv;
        (*server).blocked_on_object = ep as *mut EndpointState as *mut u8;
        enqueue(&mut ep.recv_head, &mut ep.recv_tail, server);
    }
    Err(())
}

/// Reply to the thread stored in `server.reply_tcb` with `msg`.
///
/// Wakes the caller (moves it to Ready) and clears the reply target.
/// Returns `Some(caller)` if a caller was woken, `None` if the reply target
/// was null (i.e., server was not in a call context).
///
/// # Safety
/// Must be called with the scheduler lock held.
#[cfg(not(test))]
pub unsafe fn endpoint_reply(
    server: *mut ThreadControlBlock,
    msg: &Message,
) -> Option<*mut ThreadControlBlock>
{
    let caller = unsafe { (*server).reply_tcb };
    if caller.is_null()
    {
        return None;
    }
    // Clear the reply target.
    unsafe { (*server).reply_tcb = core::ptr::null_mut(); }

    // Deliver reply to caller.
    unsafe {
        (*caller).ipc_msg = *msg;
        (*caller).state = ThreadState::Ready;
        (*caller).ipc_state = IpcThreadState::None;
        (*caller).blocked_on_object = core::ptr::null_mut();
    }
    Some(caller)
}

// ── IPC block cancellation helper ────────────────────────────────────────────

/// Remove `tcb` from a singly-linked IPC wait queue (chained through
/// `ipc_wait_next`). Updates `head`/`tail` as needed.
///
/// Returns `true` if the TCB was found and removed, `false` if not present.
///
/// Used by `SYS_THREAD_STOP` to cancel a `BlockedOnSend` or `BlockedOnRecv`.
///
/// # Safety
/// Must be called with the scheduler lock held. All pointers must be valid.
pub unsafe fn unlink_from_wait_queue(
    tcb: *mut ThreadControlBlock,
    head: &mut *mut ThreadControlBlock,
    tail: &mut *mut ThreadControlBlock,
) -> bool
{
    let mut prev: *mut ThreadControlBlock = core::ptr::null_mut();
    let mut cur = *head;

    while !cur.is_null()
    {
        if core::ptr::eq(cur, tcb)
        {
            // SAFETY: cur is a valid TCB.
            let next = unsafe { (*cur).ipc_wait_next.unwrap_or(core::ptr::null_mut()) };

            if prev.is_null()
            {
                *head = next;
            }
            else
            {
                // SAFETY: prev is a valid TCB.
                unsafe { (*prev).ipc_wait_next = if next.is_null() { None } else { Some(next) }; }
            }

            if core::ptr::eq(cur, *tail)
            {
                *tail = prev;
            }

            // SAFETY: cur is a valid TCB.
            unsafe { (*cur).ipc_wait_next = None; }
            return true;
        }

        prev = cur;
        // SAFETY: cur is a valid TCB.
        cur = unsafe { (*cur).ipc_wait_next.unwrap_or(core::ptr::null_mut()) };
    }

    false
}
