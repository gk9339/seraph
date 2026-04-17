// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// base/crasher/src/main.rs

//! Deliberate-crash test service for svcmgr monitoring validation.
//!
//! Bootstraps up to two caps from its creator (init on first start, svcmgr on
//! restarts): `caps[0]` = log endpoint, `caps[1]` = optional tokened SEND cap
//! on svcmgr's service endpoint (registered at init-time under the bundle
//! name "svcmgr"). Logs its bootstrap state, exercises the bundle cap with a
//! harmless `QUERY_ENDPOINT` probe, sleeps for 2 seconds, then triggers a
//! fault.
//!
//! This also acts as the Batch 5b validation fixture: if svcmgr's restart
//! path fails to re-inject the bundle cap, the post-restart bootstrap will
//! report `cap_count < 2` and the `QUERY_ENDPOINT` probe will be skipped,
//! making the regression visible in the log.

#![no_std]
#![no_main]

extern crate runtime;

use process_abi::StartupInfo;

#[no_mangle]
extern "Rust" fn main(startup: &StartupInfo) -> !
{
    if syscall::ipc_buffer_set(startup.ipc_buffer as u64).is_err()
    {
        syscall::thread_exit();
    }

    let mut cap_count = 0usize;
    let mut svcmgr_cap: u32 = 0;

    if startup.creator_endpoint != 0
    {
        // SAFETY: IPC buffer is registered and page-aligned.
        let ipc = unsafe { ipc::IpcBuf::from_bytes(startup.ipc_buffer) };
        if let Ok(round) = ipc::bootstrap::request_round(startup.creator_endpoint, ipc)
        {
            cap_count = round.cap_count;
            if cap_count >= 1
            {
                runtime::log::log_init(round.caps[0], startup.ipc_buffer);
            }
            if cap_count >= 2
            {
                svcmgr_cap = round.caps[1];
            }
        }
    }

    runtime::log!("crasher: alive (bootstrap caps={})", cap_count as u64);

    if svcmgr_cap != 0
    {
        probe_svcmgr(svcmgr_cap, startup.ipc_buffer);
    }

    let _ = syscall::thread_sleep(2_000);

    runtime::log!("crasher: triggering fault");

    // Trigger a fault: write to null pointer.
    // x86-64: #PF (vector 14) for unmapped page.
    // RISC-V: store page fault (scause 15).
    // SAFETY: deliberately invalid — this is the point.
    unsafe {
        core::ptr::write_volatile(core::ptr::null_mut::<u8>(), 0x42);
    }

    // SAFETY: unreachable — the write above faults and the kernel kills this thread.
    unsafe { core::hint::unreachable_unchecked() }
}

/// Liveness probe: call `QUERY_ENDPOINT` on the svcmgr cap for a name that
/// does not exist. A successful round-trip (any reply, including
/// `UNKNOWN_NAME`) proves the cap is live. A crash here would indicate the
/// bundle cap was not re-injected after restart.
fn probe_svcmgr(svcmgr_cap: u32, ipc_buffer: *mut u8)
{
    // SAFETY: IPC buffer is registered and page-aligned.
    let ipc = unsafe { ipc::IpcBuf::from_bytes(ipc_buffer) };
    let probe_name = b"__probe__";
    let name_len = probe_name.len();
    for (i, &b) in probe_name.iter().enumerate()
    {
        let word_idx = i / 8;
        let byte_idx = i % 8;
        let existing = ipc.read_word(word_idx);
        let shifted = u64::from(b) << (byte_idx * 8);
        let mask = 0xFFu64 << (byte_idx * 8);
        ipc.write_word(word_idx, (existing & !mask) | shifted);
    }
    let label = ipc::svcmgr_labels::QUERY_ENDPOINT | ((name_len as u64) << 16);
    let data_words = name_len.div_ceil(8);
    match syscall::ipc_call(svcmgr_cap, label, data_words, &[])
    {
        Ok((reply, _)) => runtime::log!("crasher: svcmgr probe reply={}", reply),
        Err(_) => runtime::log!("crasher: svcmgr probe ipc_call failed"),
    }
}
