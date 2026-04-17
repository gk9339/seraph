// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// base/allocsmoke/src/main.rs

//! Smoke test for `shared/runtime`'s `#[global_allocator]`.
//!
//! Bootstraps the per-process heap from procmgr, exercises `Box`, `Vec`,
//! `String`, and `BTreeMap` through a few basic paths, logs the results,
//! and exits cleanly. Failures panic (which `runtime`'s panic handler
//! turns into `thread_exit`), making them visible in the log.

#![no_std]
#![no_main]

extern crate alloc;
extern crate runtime;

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use process_abi::StartupInfo;

#[no_mangle]
extern "Rust" fn main(startup: &StartupInfo) -> !
{
    if syscall::ipc_buffer_set(startup.ipc_buffer as u64).is_err()
    {
        syscall::thread_exit();
    }

    // SAFETY: IPC buffer is registered and page-aligned.
    let ipc = unsafe { ipc::IpcBuf::from_bytes(startup.ipc_buffer) };

    // Bootstrap: one round, 2 caps — [log_ep, procmgr_ep].
    let mut log_ep: u32 = 0;
    let mut procmgr_ep: u32 = 0;
    if startup.creator_endpoint != 0
    {
        if let Ok(round) = ipc::bootstrap::request_round(startup.creator_endpoint, ipc)
        {
            if round.cap_count >= 1
            {
                log_ep = round.caps[0];
            }
            if round.cap_count >= 2
            {
                procmgr_ep = round.caps[1];
            }
        }
    }

    if log_ep != 0
    {
        runtime::log::log_init(log_ep, startup.ipc_buffer);
    }

    runtime::log!("allocsmoke: starting");

    if !runtime::heap::bootstrap_from_procmgr(procmgr_ep, startup.self_aspace, ipc)
    {
        runtime::log!("allocsmoke: FAIL: heap bootstrap failed");
        syscall::thread_exit();
    }
    runtime::log!("allocsmoke: heap initialised");

    // Phase 1 — Box / primitive alloc.
    let boxed: Box<u64> = Box::new(0xDEAD_BEEF_CAFE_BABE);
    runtime::log!("allocsmoke: Box<u64>={:#018x}", *boxed);

    // Phase 2 — Vec push / pop.
    let mut v: Vec<u64> = Vec::new();
    for i in 0u64..64
    {
        v.push(i);
    }
    let sum: u64 = v.iter().sum();
    runtime::log!("allocsmoke: Vec sum(0..64)={:#018x}", sum);
    let popped = v.pop().unwrap_or(0);
    runtime::log!("allocsmoke: Vec::pop={:#018x}", popped);

    // Phase 3 — String (requires heap + growing capacity).
    let mut s = String::new();
    for _ in 0..8
    {
        s.push_str("seraph ");
    }
    runtime::log!("allocsmoke: String::len={:#018x}", s.len() as u64);

    // Phase 4 — BTreeMap (exercises nested heap allocations).
    let mut m: BTreeMap<u64, u64> = BTreeMap::new();
    for k in 0u64..16
    {
        m.insert(k, k * 100);
    }
    runtime::log!("allocsmoke: BTreeMap::len={:#018x}", m.len() as u64);
    if let Some(&v10) = m.get(&10)
    {
        runtime::log!("allocsmoke: BTreeMap[10]={:#018x}", v10);
    }

    // Phase 5 — dealloc path: drop all allocations (implicit via scope).
    drop(boxed);
    drop(v);
    drop(s);
    drop(m);
    runtime::log!("allocsmoke: dealloc churn complete");

    runtime::log!("allocsmoke: PASS");
    syscall::thread_exit();
}
