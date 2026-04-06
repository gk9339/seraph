// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// ktest/src/unit/ipc.rs

//! Tier 1 tests for IPC syscalls.
//!
//! Covers: `SYS_IPC_CALL`, `SYS_IPC_REPLY`, `SYS_IPC_RECV`,
//! `SYS_IPC_BUFFER_SET`.
//!
//! `SYS_IPC_BUFFER_SET` is tested implicitly — it is called once in `run()`
//! before any tests execute, and any IPC test failure would surface a missing
//! or broken buffer. A dedicated unit test would interfere with the global
//! registration, so it is not tested in isolation here.
//!
//! The round-trip test spawns a child thread as the "caller" and uses the main
//! ktest thread as the "server". The child calls the endpoint, the server
//! receives, verifies the label, and replies.

use syscall::{
    cap_copy, cap_create_cspace, cap_create_endpoint, cap_create_signal, cap_create_thread,
    cap_delete, ipc_buffer_set, ipc_call, ipc_recv, ipc_reply, signal_send, signal_wait,
    thread_configure, thread_exit, thread_start, thread_yield,
};

use crate::{ChildStack, TestContext, TestResult};

// SEND + GRANT rights (bits 4 and 6).
const RIGHTS_SEND_GRANT: u64 = (1 << 4) | (1 << 6);

// Child stacks — one per test that spawns a child.
static mut CHILD_STACK: ChildStack = ChildStack::ZERO;
static mut RECV_BLOCKS_STACK: ChildStack = ChildStack::ZERO;

// ── SYS_IPC_CALL / SYS_IPC_RECV / SYS_IPC_REPLY ─────────────────────────────

/// Full synchronous IPC round-trip: child calls, server receives and replies.
///
/// The child sends label 0xCAFE. The server verifies the label and replies
/// with label 0xBEEF. The child verifies the reply label and signals done.
///
/// A separate sync signal (`done_sig`) lets the server wait for the child to
/// complete its post-reply verification before the test returns.
pub fn call_reply_recv(ctx: &TestContext) -> TestResult
{
    let ep = cap_create_endpoint().map_err(|_| "cap_create_endpoint for IPC test failed")?;

    // Notification signal: child sends 0xDEAD (success) or 0xBAD (failure).
    let notify =
        syscall::cap_create_signal().map_err(|_| "cap_create_signal for IPC notify failed")?;

    // Build child CSpace: endpoint (SEND | GRANT) + notify signal (SIGNAL only).
    let child_cs = cap_create_cspace(16).map_err(|_| "child CSpace create failed")?;
    let child_ep = cap_copy(ep, child_cs, RIGHTS_SEND_GRANT)
        .map_err(|_| "cap_copy ep into child CSpace failed")?;
    let child_notify = cap_copy(notify, child_cs, 1 << 7)
        .map_err(|_| "cap_copy notify into child CSpace failed")?;

    // Pack child ep and notify slots into the arg u64.
    let child_arg = (child_ep as u64) | ((child_notify as u64) << 16);

    let child_th = cap_create_thread(ctx.aspace_cap, child_cs)
        .map_err(|_| "cap_create_thread for IPC test failed")?;

    let stack_top = ChildStack::top(core::ptr::addr_of!(CHILD_STACK));
    thread_configure(
        child_th,
        caller_entry as *const () as u64,
        stack_top,
        child_arg,
    )
    .map_err(|_| "thread_configure for IPC test failed")?;
    thread_start(child_th).map_err(|_| "thread_start for IPC test failed")?;

    // Server: wait for the child's IPC call.
    let (label, _) = ipc_recv(ep).map_err(|_| "ipc_recv failed")?;
    if label != 0xCAFE
    {
        return Err("ipc_recv returned wrong label (expected 0xCAFE)");
    }

    // Reply with label 0xBEEF and no data or caps.
    ipc_reply(0xBEEF, 0, &[]).map_err(|_| "ipc_reply failed")?;

    // Wait for child confirmation.
    let result_bits = signal_wait(notify).map_err(|_| "signal_wait for IPC done failed")?;
    if result_bits != 0xDEAD
    {
        return Err("child IPC post-reply verification failed (expected 0xDEAD)");
    }

    cap_delete(child_th).map_err(|_| "cap_delete child_th after IPC test failed")?;
    cap_delete(ep).map_err(|_| "cap_delete ep after IPC test failed")?;
    cap_delete(notify).map_err(|_| "cap_delete notify after IPC test failed")?;
    cap_delete(child_cs).map_err(|_| "cap_delete child_cs after IPC test failed")?;
    Ok(())
}

// ── SYS_IPC_RECV (send-queue path) ───────────────────────────────────────────

/// Tests the send-queue path: caller blocks on the endpoint BEFORE the server
/// calls `ipc_recv`.
///
/// The server yields once after starting the child. This lets the child run,
/// call `ipc_call`, and block on the send queue before the server calls
/// `ipc_recv`.  (Contrast with `call_reply_recv`, where the server blocks first
/// and tests the recv-queue path.)
pub fn recv_finds_queued_caller(ctx: &TestContext) -> TestResult
{
    let ep = cap_create_endpoint()
        .map_err(|_| "cap_create_endpoint for recv_finds_queued_caller failed")?;
    let done =
        cap_create_signal().map_err(|_| "cap_create_signal for recv_finds_queued_caller failed")?;

    let child_cs = cap_create_cspace(16)
        .map_err(|_| "cap_create_cspace for recv_finds_queued_caller failed")?;
    let child_ep = cap_copy(ep, child_cs, RIGHTS_SEND_GRANT)
        .map_err(|_| "cap_copy ep for recv_finds_queued_caller failed")?;
    let child_done = cap_copy(done, child_cs, 1 << 7)
        .map_err(|_| "cap_copy done for recv_finds_queued_caller failed")?;
    let child_arg = (child_ep as u64) | ((child_done as u64) << 16);

    let th = cap_create_thread(ctx.aspace_cap, child_cs)
        .map_err(|_| "cap_create_thread for recv_finds_queued_caller failed")?;
    let stack_top = ChildStack::top(core::ptr::addr_of!(RECV_BLOCKS_STACK));
    thread_configure(
        th,
        queued_caller_entry as *const () as u64,
        stack_top,
        child_arg,
    )
    .map_err(|_| "thread_configure for recv_finds_queued_caller failed")?;
    thread_start(th).map_err(|_| "thread_start for recv_finds_queued_caller failed")?;

    // Yield CPU once so the child runs and blocks on ipc_call (no server yet).
    thread_yield().map_err(|_| "thread_yield for recv_finds_queued_caller failed")?;

    // Now call ipc_recv — the child should be on the send queue.
    let (label, _) = ipc_recv(ep).map_err(|_| "ipc_recv for recv_finds_queued_caller failed")?;
    if label != 0xFACE
    {
        return Err("ipc_recv returned wrong label (expected 0xFACE)");
    }

    ipc_reply(0xC0DE, 0, &[]).map_err(|_| "ipc_reply for recv_finds_queued_caller failed")?;

    let result =
        signal_wait(done).map_err(|_| "signal_wait done for recv_finds_queued_caller failed")?;
    if result != 0xDEAD
    {
        return Err("child post-reply check failed (expected 0xDEAD)");
    }

    cap_delete(th).ok();
    cap_delete(ep).ok();
    cap_delete(done).ok();
    cap_delete(child_cs).ok();
    Ok(())
}

// ── SYS_IPC_BUFFER_SET negative ──────────────────────────────────────────────

/// `ipc_buffer_set` with a non-page-aligned address must return an error.
///
/// Address 1 is obviously not page-aligned; the kernel must reject it before
/// modifying any state, so the currently registered buffer remains valid.
pub fn ipc_buffer_misaligned_err(_ctx: &TestContext) -> TestResult
{
    let err = ipc_buffer_set(1);
    if err.is_ok()
    {
        return Err("ipc_buffer_set with non-page-aligned address should fail");
    }
    Ok(())
}

// ── Child thread entry ────────────────────────────────────────────────────────

/// Child: calls the endpoint with label 0xCAFE, waits for reply, then signals.
///
/// `arg`: bits[15:0] = ep_slot, bits[31:16] = notify_slot (in child's CSpace).
fn caller_entry(arg: u64) -> !
{
    let ep_slot = (arg & 0xFFFF) as u32;
    let notify_slot = ((arg >> 16) & 0xFFFF) as u32;

    // Call the server. Blocks until server calls ipc_reply.
    match ipc_call(ep_slot, 0xCAFE, 0, &[])
    {
        Ok((reply_label, _)) =>
        {
            if reply_label == 0xBEEF
            {
                signal_send(notify_slot, 0xDEAD).ok();
            }
            else
            {
                signal_send(notify_slot, 0xBAD).ok();
            }
        }
        Err(_) =>
        {
            signal_send(notify_slot, 0xBAD).ok();
        }
    }
    thread_exit()
}

/// Child for `recv_finds_queued_caller`: calls endpoint immediately (no server
/// yet), then signals the result after the server replies.
///
/// `arg`: bits[15:0] = ep_slot, bits[31:16] = done_slot (in child's CSpace).
fn queued_caller_entry(arg: u64) -> !
{
    let ep_slot = (arg & 0xFFFF) as u32;
    let done_slot = ((arg >> 16) & 0xFFFF) as u32;

    // ipc_call with no server yet — blocks on the endpoint's send queue.
    match ipc_call(ep_slot, 0xFACE, 0, &[])
    {
        Ok((reply_label, _)) =>
        {
            let result = if reply_label == 0xC0DE { 0xDEAD } else { 0xBAD };
            signal_send(done_slot, result).ok();
        }
        Err(_) =>
        {
            signal_send(done_slot, 0xBAD).ok();
        }
    }
    thread_exit()
}
