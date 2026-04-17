// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// svcmgr/src/main.rs

//! Seraph service manager — monitors services, detects crashes via death
//! notification event queues, and restarts them per their restart policy.
//!
//! svcmgr is loaded from the root filesystem by init (via procmgr's
//! `CREATE_PROCESS_FROM_VFS`). Init registers services via IPC, then sends
//! `HANDOVER_COMPLETE` and exits. svcmgr runs for the lifetime of the system.
//!
//! See `svcmgr/docs/ipc-interface.md` and `svcmgr/docs/restart-protocol.md`.

#![no_std]
#![no_main]
// cast_possible_truncation: targets 64-bit only; u64/usize conversions lossless.
#![allow(clippy::cast_possible_truncation)]

extern crate runtime;

mod restart;
mod service;

use ipc::{svcmgr_labels, IpcBuf};
use process_abi::StartupInfo;
use service::{bootstrap_caps, ServiceEntry, MAX_SERVICES};

// ── Registration handling ──────────────────────────────────────────────────

/// Handle a `REGISTER_SERVICE` IPC message.
///
/// Reads name, policy, criticality from data words. Reads `thread_cap`,
/// `module_cap`, `log_ep` from transferred caps. Creates an event queue, binds
/// it to the thread, adds to the wait set.
#[allow(clippy::too_many_lines)]
fn handle_register(
    ipc_buf: *mut u64,
    label: u64,
    services: &mut [ServiceEntry; MAX_SERVICES],
    service_count: &mut usize,
    ws_cap: u32,
) -> u64
{
    let name_len = ((label >> 16) & 0xFFFF) as usize;
    if name_len == 0 || name_len > 32
    {
        return ipc::svcmgr_errors::INVALID_NAME;
    }
    if *service_count >= MAX_SERVICES
    {
        return ipc::svcmgr_errors::TABLE_FULL;
    }

    // SAFETY: ipc_buf is the registered IPC buffer page.
    let ipc = unsafe { ipc::IpcBuf::from_raw(ipc_buf) };
    let restart_policy = ipc.read_word(0) as u8;
    let criticality = ipc.read_word(1) as u8;

    let name = read_name_from_ipc(ipc_buf, name_len);

    // Read transferred caps: cap[0]=thread, cap[1]=module, cap[2]=log_ep (optional).
    // SAFETY: ipc_buf is the registered IPC buffer, page-aligned.
    let (cap_count, recv_caps) = unsafe { syscall::read_recv_caps(ipc_buf.cast::<u64>()) };

    if cap_count < 2
    {
        return ipc::svcmgr_errors::INSUFFICIENT_CAPS;
    }

    let thread_cap = recv_caps[0];
    let module_cap = recv_caps[1];
    let log_ep_cap = if cap_count >= 3 { recv_caps[2] } else { 0 };

    if thread_cap == 0 || module_cap == 0
    {
        return ipc::svcmgr_errors::INSUFFICIENT_CAPS;
    }

    let Some(eq_cap) = create_and_bind_event_queue(thread_cap, ws_cap, *service_count)
    else
    {
        return ipc::svcmgr_errors::EVENT_QUEUE_FAILED;
    };

    let idx = *service_count;
    services[idx] = ServiceEntry {
        name,
        name_len: name_len as u8,
        thread_cap,
        module_cap,
        log_ep_cap,
        restart_policy,
        criticality,
        event_queue_cap: eq_cap,
        restart_count: 0,
        active: true,
        bootstrap_token: 0,
    };
    *service_count += 1;

    runtime::log!("svcmgr: registered service: {}", services[idx].name_str());

    ipc::svcmgr_errors::SUCCESS
}

/// Read a service name from IPC buffer data words starting at offset 2.
fn read_name_from_ipc(ipc_buf: *mut u64, name_len: usize) -> [u8; 32]
{
    // SAFETY: ipc_buf is the registered IPC buffer page.
    let ipc = unsafe { ipc::IpcBuf::from_raw(ipc_buf) };
    let mut name = [0u8; 32];
    let name_words = name_len.div_ceil(8);
    for w in 0..name_words
    {
        let word = ipc.read_word(2 + w);
        for b in 0..8
        {
            let idx = w * 8 + b;
            if idx < name_len
            {
                name[idx] = (word >> (b * 8)) as u8;
            }
        }
    }
    name
}

/// Create an event queue, bind it to a thread for death notification, and add
/// it to the wait set. Returns the event queue cap on success.
fn create_and_bind_event_queue(thread_cap: u32, ws_cap: u32, service_index: usize) -> Option<u32>
{
    let Ok(eq_cap) = syscall::event_queue_create(4)
    else
    {
        runtime::log!("svcmgr: failed to create event queue for service");
        return None;
    };

    if syscall::thread_bind_notification(thread_cap, eq_cap).is_err()
    {
        runtime::log!("svcmgr: failed to bind death notification");
        return None;
    }

    // Token = service_index + 1 (token 0 = service endpoint).
    let token = (service_index as u64) + 1;
    if syscall::wait_set_add(ws_cap, eq_cap, token).is_err()
    {
        runtime::log!("svcmgr: failed to add event queue to wait set");
        return None;
    }

    Some(eq_cap)
}

// ── Halt ───────────────────────────────────────────────────────────────────

/// Halt the CPU in an infinite loop. Used on unrecoverable failures.
pub fn halt_loop() -> !
{
    loop
    {
        #[cfg(target_arch = "x86_64")]
        // SAFETY: hlt halts CPU until next interrupt.
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack));
        }

        #[cfg(target_arch = "riscv64")]
        // SAFETY: wfi waits for interrupt.
        unsafe {
            core::arch::asm!("wfi", options(nomem, nostack));
        }
    }
}

// ── Entry point ────────────────────────────────────────────────────────────

#[allow(clippy::too_many_lines)]
#[no_mangle]
extern "Rust" fn main(startup: &StartupInfo) -> !
{
    // Register IPC buffer.
    if syscall::ipc_buffer_set(startup.ipc_buffer as u64).is_err()
    {
        syscall::thread_exit();
    }

    // SAFETY: IPC buffer is registered and page-aligned.
    let ipc = unsafe { IpcBuf::from_bytes(startup.ipc_buffer) };
    let ipc_buf = ipc.as_ptr();

    let Some(caps) = bootstrap_caps(startup, ipc)
    else
    {
        syscall::thread_exit();
    };

    if caps.log_ep != 0
    {
        runtime::log::log_init(caps.log_ep, startup.ipc_buffer);
    }

    runtime::log!("svcmgr: started");

    if caps.service_ep == 0
    {
        runtime::log!("svcmgr: no service endpoint, halting");
        halt_loop();
    }
    if caps.procmgr_ep == 0
    {
        runtime::log!("svcmgr: no procmgr endpoint, halting");
        halt_loop();
    }

    let Ok(ws_cap) = syscall::wait_set_create()
    else
    {
        runtime::log!("svcmgr: failed to create wait set");
        halt_loop();
    };

    if syscall::wait_set_add(ws_cap, caps.service_ep, 0).is_err()
    {
        runtime::log!("svcmgr: failed to add service endpoint to wait set");
        halt_loop();
    }

    let mut services = [const { ServiceEntry::empty() }; MAX_SERVICES];
    let mut service_count: usize = 0;
    let mut handover_complete = false;

    runtime::log!("svcmgr: waiting for registrations");

    event_loop(
        &caps,
        ws_cap,
        ipc,
        ipc_buf,
        &mut services,
        &mut service_count,
        &mut handover_complete,
    );
}

/// Main event loop: dispatches IPC registrations and death notifications.
#[allow(clippy::too_many_arguments)]
fn event_loop(
    caps: &service::SvcmgrCaps,
    ws_cap: u32,
    ipc: IpcBuf,
    ipc_buf: *mut u64,
    services: &mut [ServiceEntry; MAX_SERVICES],
    service_count: &mut usize,
    handover_complete: &mut bool,
) -> !
{
    loop
    {
        let Ok(token) = syscall::wait_set_wait(ws_cap)
        else
        {
            runtime::log!("svcmgr: wait_set_wait failed");
            continue;
        };

        if token == 0
        {
            dispatch_ipc(
                caps.service_ep,
                ipc_buf,
                services,
                service_count,
                ws_cap,
                handover_complete,
            );
        }
        else
        {
            dispatch_death(token, services, *service_count, caps, ws_cap, ipc);
        }
    }
}

/// Handle an IPC message on the service endpoint (registration or handover).
fn dispatch_ipc(
    service_ep: u32,
    ipc_buf: *mut u64,
    services: &mut [ServiceEntry; MAX_SERVICES],
    service_count: &mut usize,
    ws_cap: u32,
    handover_complete: &mut bool,
)
{
    let Ok((label, _data_count)) = syscall::ipc_recv(service_ep)
    else
    {
        return;
    };

    let opcode = label & 0xFFFF;
    match opcode
    {
        svcmgr_labels::REGISTER_SERVICE =>
        {
            let result = handle_register(ipc_buf, label, services, service_count, ws_cap);
            let _ = syscall::ipc_reply(result, 0, &[]);
        }
        svcmgr_labels::HANDOVER_COMPLETE =>
        {
            *handover_complete = true;
            let _ = syscall::ipc_reply(ipc::svcmgr_errors::SUCCESS, 0, &[]);
            runtime::log!(
                "svcmgr: handover complete, monitoring services: {:#018x}",
                *service_count as u64
            );
        }
        _ =>
        {
            let _ = syscall::ipc_reply(ipc::svcmgr_errors::UNKNOWN_OPCODE, 0, &[]);
        }
    }
}

/// Handle a death notification from a monitored service.
fn dispatch_death(
    token: u64,
    services: &mut [ServiceEntry; MAX_SERVICES],
    service_count: usize,
    caps: &service::SvcmgrCaps,
    ws_cap: u32,
    ipc: IpcBuf,
)
{
    let idx = (token - 1) as usize;
    if idx >= service_count
    {
        runtime::log!("svcmgr: invalid death notification token");
        return;
    }

    let Ok(exit_reason) = syscall::event_recv(services[idx].event_queue_cap)
    else
    {
        runtime::log!("svcmgr: event_recv failed");
        return;
    };

    if !services[idx].active
    {
        return;
    }

    restart::handle_death(
        &mut services[idx],
        exit_reason,
        caps.procmgr_ep,
        caps.bootstrap_ep,
        ipc,
        ws_cap,
    );

    // Re-add event queue to wait set with same token (if still active).
    if services[idx].active
        && syscall::wait_set_add(ws_cap, services[idx].event_queue_cap, token).is_err()
    {
        runtime::log!("svcmgr: failed to re-add event queue to wait set after restart");
        services[idx].active = false;
    }
}
