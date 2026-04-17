// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// vfsd/src/worker.rs

//! Bootstrap worker thread for fatfs children.
//!
//! vfsd's main thread holds `reply_tcb = init` while servicing an init-issued
//! MOUNT. The kernel's single-slot reply-target prohibits nested server IPC —
//! a `serve_round` on vfsd's main thread would clobber that outer reply target.
//! Offloading bootstrap delivery to this worker thread keeps the main thread's
//! reply path intact so fatfs can participate in the generic bootstrap protocol
//! like every other service.
//!
//! The worker owns a dedicated endpoint (`bootstrap_ep`). Each new fatfs child
//! is spawned with a tokened SEND cap on that endpoint as its creator endpoint.
//! The main thread publishes a plan keyed by token, then `signal_wait`s on
//! `done_sig`. The worker reads the plan, delivers the round, and signals main.

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use ipc::{bootstrap, bootstrap_errors, IpcBuf};

/// Worker-thread stack base VA.
pub const STACK_BASE: u64 = 0x0000_0000_D000_0000;
/// Worker-thread stack size in pages (8 KiB).
pub const STACK_PAGES: u64 = 2;
/// Worker-thread stack top (exclusive upper bound).
pub const STACK_TOP: u64 = STACK_BASE + STACK_PAGES * 4096;
/// Worker-thread IPC buffer VA (one page above the stack).
pub const IPC_BUF_VA: u64 = 0x0000_0000_D001_0000;

/// Active plan token. Nonzero while a plan is pending; zeroed by the worker
/// after delivery so stale `REQUEST`s reusing the same token get `NO_CHILD`.
pub static PLAN_TOKEN: AtomicU64 = AtomicU64::new(0);

/// Block-device SEND cap for the pending plan.
pub static PLAN_BLK: AtomicU32 = AtomicU32::new(0);
/// Log-endpoint SEND cap for the pending plan (0 if none).
pub static PLAN_LOG: AtomicU32 = AtomicU32::new(0);
/// fatfs service-endpoint cap (`RIGHTS_ALL`) for the pending plan.
pub static PLAN_SERVICE: AtomicU32 = AtomicU32::new(0);
/// Partition base LBA delivered as the single data word.
pub static PLAN_LBA: AtomicU64 = AtomicU64::new(0);

/// Bootstrap endpoint (worker-owned). Populated before thread start.
pub static BOOTSTRAP_EP: AtomicU32 = AtomicU32::new(0);
/// Signal cap the worker raises when a plan has been delivered.
pub static DONE_SIG: AtomicU32 = AtomicU32::new(0);

/// Worker thread entry point. Never returns.
pub extern "C" fn entry(_arg: u64) -> !
{
    if syscall::ipc_buffer_set(IPC_BUF_VA).is_err()
    {
        syscall::thread_exit();
    }

    // SAFETY: IPC_BUF_VA is registered above and is a page-aligned VA mapped
    // writable before thread start.
    let ipc = unsafe { IpcBuf::from_raw(IPC_BUF_VA as *mut u64) };

    let bootstrap_ep = BOOTSTRAP_EP.load(Ordering::Acquire);
    let done_sig = DONE_SIG.load(Ordering::Acquire);
    if bootstrap_ep == 0 || done_sig == 0
    {
        syscall::thread_exit();
    }

    loop
    {
        let Ok((label, token)) = syscall::ipc_recv(bootstrap_ep)
        else
        {
            continue;
        };

        let plan_token = PLAN_TOKEN.load(Ordering::Acquire);
        if token == 0 || token != plan_token
        {
            let _ = bootstrap::reply_error(bootstrap_errors::NO_CHILD);
            continue;
        }
        if (label & 0xFFFF) != bootstrap::REQUEST
        {
            let _ = bootstrap::reply_error(bootstrap_errors::INVALID);
            continue;
        }

        let blk = PLAN_BLK.load(Ordering::Relaxed);
        let log = PLAN_LOG.load(Ordering::Relaxed);
        let service = PLAN_SERVICE.load(Ordering::Relaxed);
        let lba = PLAN_LBA.load(Ordering::Relaxed);

        // Invalidate the plan token before replying so a duplicate REQUEST
        // cannot drain the same caps twice (caps are moved by ipc_reply).
        PLAN_TOKEN.store(0, Ordering::Release);

        ipc.write_word(0, lba);
        let caps = [blk, log, service];
        if bootstrap::reply_round(true, &caps, 1).is_err()
        {
            // Reply failed — signal so the main thread isn't left waiting.
            let _ = syscall::signal_send(done_sig, 2);
            continue;
        }

        let _ = syscall::signal_send(done_sig, 1);
    }
}

/// Publish a plan for the next fatfs child. Main thread only.
pub fn publish_plan(token: u64, blk: u32, log: u32, service: u32, lba: u64)
{
    PLAN_BLK.store(blk, Ordering::Relaxed);
    PLAN_LOG.store(log, Ordering::Relaxed);
    PLAN_SERVICE.store(service, Ordering::Relaxed);
    PLAN_LBA.store(lba, Ordering::Relaxed);
    // Release: ensures the worker's Acquire load of PLAN_TOKEN synchronises
    // with the other fields being visible.
    PLAN_TOKEN.store(token, Ordering::Release);
}
