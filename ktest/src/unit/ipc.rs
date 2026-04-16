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
    cap_delete, cap_derive, ipc_buffer_set, ipc_call, ipc_recv, ipc_reply, signal_send,
    signal_wait, thread_configure, thread_exit, thread_start, thread_yield,
};

use crate::{ChildStack, TestContext, TestResult};

// SEND + GRANT rights (bits 4 and 6).
const RIGHTS_SEND_GRANT: u64 = (1 << 4) | (1 << 6);
// RECV right only (bit 4 for SEND is not set).
const RIGHTS_RECV_ONLY: u64 = 1 << 10;

// Child stacks — one per test that spawns a child.
static mut CHILD_STACK: ChildStack = ChildStack::ZERO;
static mut RECV_BLOCKS_STACK: ChildStack = ChildStack::ZERO;
static mut DATA_WORDS_STACK: ChildStack = ChildStack::ZERO;
static mut CAP_XFER_STACK: ChildStack = ChildStack::ZERO;
static mut TOKEN_STACK: ChildStack = ChildStack::ZERO;

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
    let child_arg = u64::from(child_ep) | (u64::from(child_notify) << 16);

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
    let child_arg = u64::from(child_ep) | (u64::from(child_done) << 16);

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

// ── SYS_IPC_CALL (insufficient rights) ───────────────────────────────────────

/// `ipc_call` on an endpoint cap with only RECV right (no SEND) must fail.
pub fn send_insufficient_rights_err(_ctx: &TestContext) -> TestResult
{
    let ep =
        cap_create_endpoint().map_err(|_| "cap_create_endpoint for send_rights test failed")?;

    // Derive with RECV right only (bit 10), no SEND (bit 4).
    let recv_only =
        cap_derive(ep, RIGHTS_RECV_ONLY).map_err(|_| "cap_derive for send_rights test failed")?;

    // ipc_call requires SEND right.
    let err = ipc_call(recv_only, 0xABCD, 0, &[]);
    if err.is_ok()
    {
        return Err("ipc_call on RECV-only cap should fail (InsufficientRights)");
    }

    cap_delete(recv_only).map_err(|_| "cap_delete recv_only failed")?;
    cap_delete(ep).map_err(|_| "cap_delete ep after send_rights test failed")?;
    Ok(())
}

// ── SYS_IPC_CALL with data words ─────────────────────────────────────────────

/// IPC call with `data_count`=2 transfers data words via the IPC buffer.
///
/// The child writes two data words into its IPC buffer before calling.
/// The server receives them and verifies the values.
pub fn call_with_data_words(ctx: &TestContext) -> TestResult
{
    let ep = cap_create_endpoint().map_err(|_| "cap_create_endpoint for data_words test failed")?;
    let done = cap_create_signal().map_err(|_| "cap_create_signal for data_words test failed")?;

    let child_cs =
        cap_create_cspace(16).map_err(|_| "cap_create_cspace for data_words test failed")?;
    let child_ep = cap_copy(ep, child_cs, RIGHTS_SEND_GRANT)
        .map_err(|_| "cap_copy ep for data_words test failed")?;
    let child_done =
        cap_copy(done, child_cs, 1 << 7).map_err(|_| "cap_copy done for data_words test failed")?;
    let child_arg = u64::from(child_ep) | (u64::from(child_done) << 16);

    let child_th = cap_create_thread(ctx.aspace_cap, child_cs)
        .map_err(|_| "cap_create_thread for data_words test failed")?;
    let stack_top = ChildStack::top(core::ptr::addr_of!(DATA_WORDS_STACK));
    thread_configure(
        child_th,
        data_caller_entry as *const () as u64,
        stack_top,
        child_arg,
    )
    .map_err(|_| "thread_configure for data_words test failed")?;
    thread_start(child_th).map_err(|_| "thread_start for data_words test failed")?;

    // Server: receive the call.
    let (label, _) = ipc_recv(ep).map_err(|_| "ipc_recv for data_words test failed")?;
    if label != 0xDA7A
    {
        return Err("ipc_recv returned wrong label for data_words test");
    }

    // Read data words from our IPC buffer.
    // SAFETY: ctx.ipc_buf is the registered IPC buffer (4 KiB, page-aligned);
    // kernel wrote data words at indices 0 and 1 during ipc_recv; volatile reads
    // required for kernel-written data.
    let (word0, word1) = unsafe {
        (
            core::ptr::read_volatile(ctx.ipc_buf),
            core::ptr::read_volatile(ctx.ipc_buf.add(1)),
        )
    };

    ipc_reply(0, 0, &[]).map_err(|_| "ipc_reply for data_words test failed")?;

    signal_wait(done).map_err(|_| "signal_wait for data_words test failed")?;

    if word0 != 0xAAAA_BBBB
    {
        return Err("data word[0] mismatch (expected 0xAAAABBBB)");
    }
    if word1 != 0xCCCC_DDDD
    {
        return Err("data word[1] mismatch (expected 0xCCCCDDDD)");
    }

    cap_delete(child_th).ok();
    cap_delete(ep).ok();
    cap_delete(done).ok();
    cap_delete(child_cs).ok();
    Ok(())
}

// ── SYS_IPC_CALL with cap transfer ───────────────────────────────────────────

/// IPC call transferring one capability from caller to server.
///
/// The child creates a signal, passes it via the IPC cap transfer mechanism.
/// The server receives it and verifies it can use the transferred cap.
pub fn call_with_cap_transfer(ctx: &TestContext) -> TestResult
{
    let ep = cap_create_endpoint().map_err(|_| "cap_create_endpoint for cap_xfer test failed")?;
    let done = cap_create_signal().map_err(|_| "cap_create_signal for cap_xfer test failed")?;

    let child_cs =
        cap_create_cspace(16).map_err(|_| "cap_create_cspace for cap_xfer test failed")?;
    let child_ep = cap_copy(ep, child_cs, RIGHTS_SEND_GRANT)
        .map_err(|_| "cap_copy ep for cap_xfer test failed")?;
    let child_done =
        cap_copy(done, child_cs, 1 << 7).map_err(|_| "cap_copy done for cap_xfer test failed")?;
    let child_arg = u64::from(child_ep) | (u64::from(child_done) << 16);

    let child_th = cap_create_thread(ctx.aspace_cap, child_cs)
        .map_err(|_| "cap_create_thread for cap_xfer test failed")?;
    let stack_top = ChildStack::top(core::ptr::addr_of!(CAP_XFER_STACK));
    thread_configure(
        child_th,
        cap_xfer_caller_entry as *const () as u64,
        stack_top,
        child_arg,
    )
    .map_err(|_| "thread_configure for cap_xfer test failed")?;
    thread_start(child_th).map_err(|_| "thread_start for cap_xfer test failed")?;

    // Server: receive the call with cap transfer.
    let (label, _) = ipc_recv(ep).map_err(|_| "ipc_recv for cap_xfer test failed")?;
    if label != 0xCAFE
    {
        return Err("ipc_recv returned wrong label for cap_xfer test");
    }

    // Read cap transfer results from IPC buffer.
    // SAFETY: ctx.ipc_buf points to the registered IPC buffer.
    let (cap_count, cap_indices) = unsafe { syscall::read_recv_caps(ctx.ipc_buf) };
    if cap_count != 1
    {
        return Err("expected 1 transferred cap, got different count");
    }

    // The transferred cap should be a valid signal — try sending on it.
    let transferred_sig = cap_indices[0];
    let send_result = syscall::signal_send(transferred_sig, 0x1);

    ipc_reply(0, 0, &[]).map_err(|_| "ipc_reply for cap_xfer test failed")?;

    signal_wait(done).map_err(|_| "signal_wait for cap_xfer test failed")?;

    if send_result.is_err()
    {
        return Err("transferred cap is not usable as a signal");
    }

    // Clean up the transferred cap.
    cap_delete(transferred_sig).ok();
    cap_delete(child_th).ok();
    cap_delete(ep).ok();
    cap_delete(done).ok();
    cap_delete(child_cs).ok();
    Ok(())
}

// ── Token delivery via IPC ───────────────────────────────────────────────────

/// `ipc_recv` delivers the token from the sender's endpoint cap.
///
/// The child calls via a tokened endpoint cap (token=0x1234). The server
/// receives and verifies the token value in the third return register.
pub fn recv_delivers_token(ctx: &TestContext) -> TestResult
{
    let ep =
        cap_create_endpoint().map_err(|_| "cap_create_endpoint for recv_delivers_token failed")?;
    let done =
        cap_create_signal().map_err(|_| "cap_create_signal for recv_delivers_token failed")?;

    // Derive a tokened send+grant cap.
    let tokened_ep = syscall::cap_derive_token(ep, RIGHTS_SEND_GRANT, 0x1234)
        .map_err(|_| "cap_derive_token for recv_delivers_token failed")?;

    let child_cs =
        cap_create_cspace(16).map_err(|_| "cap_create_cspace for recv_delivers_token failed")?;
    let child_ep = cap_copy(tokened_ep, child_cs, syscall::RIGHTS_ALL)
        .map_err(|_| "cap_copy tokened ep for recv_delivers_token failed")?;
    let child_done = cap_copy(done, child_cs, 1 << 7)
        .map_err(|_| "cap_copy done for recv_delivers_token failed")?;
    let child_arg = u64::from(child_ep) | (u64::from(child_done) << 16);

    let th = cap_create_thread(ctx.aspace_cap, child_cs)
        .map_err(|_| "cap_create_thread for recv_delivers_token failed")?;

    let stack_top = ChildStack::top(core::ptr::addr_of!(TOKEN_STACK));
    thread_configure(
        th,
        token_caller_entry as *const () as u64,
        stack_top,
        child_arg,
    )
    .map_err(|_| "thread_configure for recv_delivers_token failed")?;
    thread_start(th).map_err(|_| "thread_start for recv_delivers_token failed")?;

    // Server: receive and check token.
    let (label, token) = ipc_recv(ep).map_err(|_| "ipc_recv for recv_delivers_token failed")?;

    if label != 0xD00D
    {
        return Err("recv_delivers_token: wrong label (expected 0xD00D)");
    }
    if token != 0x1234
    {
        return Err("recv_delivers_token: wrong token (expected 0x1234)");
    }

    // Reply so child can finish.
    ipc_reply(0, 0, &[]).map_err(|_| "ipc_reply for recv_delivers_token failed")?;

    signal_wait(done).map_err(|_| "signal_wait for recv_delivers_token failed")?;

    cap_delete(th).ok();
    cap_delete(tokened_ep).ok();
    cap_delete(ep).ok();
    cap_delete(done).ok();
    cap_delete(child_cs).ok();
    Ok(())
}

/// `ipc_recv` returns token=0 when the sender uses an untokened cap.
pub fn recv_untokened_returns_zero(ctx: &TestContext) -> TestResult
{
    let ep = cap_create_endpoint().map_err(|_| "cap_create_endpoint for recv_untokened failed")?;
    let done = cap_create_signal().map_err(|_| "cap_create_signal for recv_untokened failed")?;

    // Give child an untokened send+grant cap (regular derive, no token).
    let child_cs =
        cap_create_cspace(16).map_err(|_| "cap_create_cspace for recv_untokened failed")?;
    let child_ep = cap_copy(ep, child_cs, RIGHTS_SEND_GRANT)
        .map_err(|_| "cap_copy ep for recv_untokened failed")?;
    let child_done =
        cap_copy(done, child_cs, 1 << 7).map_err(|_| "cap_copy done for recv_untokened failed")?;
    let child_arg = u64::from(child_ep) | (u64::from(child_done) << 16);

    let th = cap_create_thread(ctx.aspace_cap, child_cs)
        .map_err(|_| "cap_create_thread for recv_untokened failed")?;

    // Reuse the caller_entry (sends 0xCAFE, expects reply 0xBEEF).
    let stack_top = ChildStack::top(core::ptr::addr_of!(CHILD_STACK));
    thread_configure(th, caller_entry as *const () as u64, stack_top, child_arg)
        .map_err(|_| "thread_configure for recv_untokened failed")?;
    thread_start(th).map_err(|_| "thread_start for recv_untokened failed")?;

    let (label, token) = ipc_recv(ep).map_err(|_| "ipc_recv for recv_untokened failed")?;

    if label != 0xCAFE
    {
        return Err("recv_untokened: wrong label");
    }
    if token != 0
    {
        return Err("recv_untokened: token should be 0 for untokened cap");
    }

    ipc_reply(0xBEEF, 0, &[]).map_err(|_| "ipc_reply for recv_untokened failed")?;

    signal_wait(done).map_err(|_| "signal_wait for recv_untokened failed")?;

    cap_delete(th).ok();
    cap_delete(ep).ok();
    cap_delete(done).ok();
    cap_delete(child_cs).ok();
    Ok(())
}

// ── Child thread entry ────────────────────────────────────────────────────────

/// Child: calls the endpoint with label 0xCAFE, waits for reply, then signals.
///
/// `arg`: bits[15:0] = `ep_slot`, bits[31:16] = `notify_slot` (in child's `CSpace`).
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
/// `arg`: bits[15:0] = `ep_slot`, bits[31:16] = `done_slot` (in child's `CSpace`).
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

/// Child for `call_with_data_words`: registers its IPC buffer, writes two data
/// words, then calls with `data_count`=2.
///
/// `arg`: bits[15:0] = `ep_slot`, bits[31:16] = `done_slot`.
fn data_caller_entry(arg: u64) -> !
{
    let ep_slot = (arg & 0xFFFF) as u32;
    let done_slot = ((arg >> 16) & 0xFFFF) as u32;

    // Register the shared IPC buffer for this child thread. Each thread has its
    // own IPC buffer pointer in its TCB; the child must register before calling.
    let buf_addr = core::ptr::addr_of!(crate::IPC_BUF) as u64;
    if syscall::ipc_buffer_set(buf_addr).is_err()
    {
        signal_send(done_slot, 0xBAD).ok();
        thread_exit()
    }

    // Write data words to the IPC buffer before calling.
    // SAFETY: IPC_BUF is page-aligned and within our address space.
    unsafe {
        let buf = buf_addr as *mut u64;
        core::ptr::write_volatile(buf, 0xAAAA_BBBB);
        core::ptr::write_volatile(buf.add(1), 0xCCCC_DDDD);
    }

    match ipc_call(ep_slot, 0xDA7A, 2, &[])
    {
        Ok(_) => signal_send(done_slot, 0xDEAD).ok(),
        Err(_) => signal_send(done_slot, 0xBAD).ok(),
    };
    thread_exit()
}

/// Child for `call_with_cap_transfer`: creates a signal and transfers it via IPC.
///
/// `arg`: bits[15:0] = `ep_slot`, bits[31:16] = `done_slot`.
fn cap_xfer_caller_entry(arg: u64) -> !
{
    let ep_slot = (arg & 0xFFFF) as u32;
    let done_slot = ((arg >> 16) & 0xFFFF) as u32;

    // Register IPC buffer for cap transfer.
    let buf_addr = core::ptr::addr_of!(crate::IPC_BUF) as u64;
    if syscall::ipc_buffer_set(buf_addr).is_err()
    {
        signal_send(done_slot, 0xBAD).ok();
        thread_exit()
    }

    // Create a signal in the child's CSpace.
    let Ok(sig) = syscall::cap_create_signal()
    else
    {
        signal_send(done_slot, 0xBAD).ok();
        thread_exit()
    };

    // Call with 1 cap to transfer.
    match ipc_call(ep_slot, 0xCAFE, 0, &[sig])
    {
        Ok(_) => signal_send(done_slot, 0xDEAD).ok(),
        Err(_) => signal_send(done_slot, 0xBAD).ok(),
    };
    thread_exit()
}

/// Child for `recv_delivers_token`: calls endpoint with label 0xD00D.
///
/// `arg`: bits[15:0] = `ep_slot`, bits[31:16] = `done_slot`.
fn token_caller_entry(arg: u64) -> !
{
    let ep_slot = (arg & 0xFFFF) as u32;
    let done_slot = ((arg >> 16) & 0xFFFF) as u32;

    match ipc_call(ep_slot, 0xD00D, 0, &[])
    {
        Ok(_) => signal_send(done_slot, 0xDEAD).ok(),
        Err(_) => signal_send(done_slot, 0xBAD).ok(),
    };
    thread_exit()
}
