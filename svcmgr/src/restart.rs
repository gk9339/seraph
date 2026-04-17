// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// svcmgr/src/restart.rs

//! Service death handling and restart logic.
//!
//! Detects whether a crashed service should be restarted based on its restart
//! policy and criticality, then creates a new process instance via procmgr,
//! serves its bootstrap (log endpoint only for now — the current set of
//! restart-eligible services are single-cap crasher-class processes), and
//! rebinds death notification.

use crate::halt_loop;
use crate::service::{
    ServiceEntry, CRITICALITY_FATAL, CRITICALITY_NORMAL, MAX_RESTARTS, POLICY_ALWAYS,
    POLICY_ON_FAILURE,
};
use ipc::{procmgr_labels, IpcBuf};

/// Monotonic counter for restart-child bootstrap tokens.
static NEXT_BOOTSTRAP_TOKEN: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(1);

/// Handle a service death detected via event queue notification.
///
/// Checks criticality and restart policy, then attempts to restart the service
/// if appropriate. Marks the service inactive if restart is not attempted or
/// fails.
#[allow(clippy::too_many_arguments)]
pub fn handle_death(
    svc: &mut ServiceEntry,
    exit_reason: u64,
    procmgr_ep: u32,
    bootstrap_ep: u32,
    ipc: IpcBuf,
    ws_cap: u32,
)
{
    runtime::log!("svcmgr: service died: {}", svc.name_str());
    runtime::log!("svcmgr:   exit_reason={:#018x}", exit_reason);

    if svc.criticality == CRITICALITY_FATAL
    {
        runtime::log!("svcmgr: FATAL service crashed, halting");
        halt_loop();
    }

    if svc.criticality != CRITICALITY_NORMAL
    {
        runtime::log!("svcmgr: unknown criticality, not restarting");
        svc.active = false;
        return;
    }

    if !should_restart(svc, exit_reason)
    {
        svc.active = false;
        return;
    }

    runtime::log!(
        "svcmgr: restarting (attempt {:#018x})",
        u64::from(svc.restart_count + 1)
    );

    if !restart_process(svc, procmgr_ep, bootstrap_ep, ipc, ws_cap)
    {
        svc.active = false;
        return;
    }

    svc.restart_count += 1;
    runtime::log!("svcmgr: service restarted: {}", svc.name_str());
}

/// Determine whether a service should be restarted based on its policy and
/// restart count.
fn should_restart(svc: &ServiceEntry, exit_reason: u64) -> bool
{
    let restart = match svc.restart_policy
    {
        POLICY_ALWAYS => true,
        POLICY_ON_FAILURE => exit_reason >= syscall_abi::EXIT_FAULT_BASE,
        _ => false,
    };

    if !restart
    {
        runtime::log!("svcmgr: restart policy says no restart");
        return false;
    }

    if svc.restart_count >= MAX_RESTARTS
    {
        runtime::log!("svcmgr: max restarts reached, marking degraded");
        return false;
    }

    if svc.module_cap == 0
    {
        runtime::log!("svcmgr: no module cap, cannot restart");
        return false;
    }

    true
}

/// Create a new process via procmgr, serve bootstrap (log endpoint), start it,
/// and rebind death notification. Returns `true` on success.
fn restart_process(
    svc: &mut ServiceEntry,
    procmgr_ep: u32,
    bootstrap_ep: u32,
    ipc: IpcBuf,
    ws_cap: u32,
) -> bool
{
    let Some((process_handle, new_thread_cap, child_token)) =
        create_process(svc, procmgr_ep, bootstrap_ep, ipc)
    else
    {
        return false;
    };

    // Start the new process.
    if !start_process(process_handle)
    {
        return false;
    }

    // Serve crasher's single-round bootstrap: one cap (log_ep), done=true.
    let log_cap = if svc.log_ep_cap != 0
    {
        if let Ok(c) = syscall::cap_derive(svc.log_ep_cap, syscall::RIGHTS_SEND)
        {
            c
        }
        else
        {
            runtime::log!("svcmgr: cannot derive log cap for restart");
            return false;
        }
    }
    else
    {
        0
    };

    let caps_slice: &[u32] = if log_cap != 0 { &[log_cap] } else { &[] };
    if ipc::bootstrap::serve_round(bootstrap_ep, child_token, ipc, true, caps_slice, &[]).is_err()
    {
        runtime::log!("svcmgr: bootstrap serve failed");
        return false;
    }

    svc.bootstrap_token = child_token;

    rebind_death_notification(svc, ws_cap, new_thread_cap)
}

/// Send `CREATE_PROCESS` to procmgr. Returns `(process_handle, thread, child_token)`.
fn create_process(
    svc: &ServiceEntry,
    procmgr_ep: u32,
    bootstrap_ep: u32,
    ipc: IpcBuf,
) -> Option<(u32, u32, u64)>
{
    let module_copy = syscall::cap_derive(svc.module_cap, syscall::RIGHTS_ALL).ok()?;

    // Allocate a fresh bootstrap token for this child.
    let child_token = NEXT_BOOTSTRAP_TOKEN.fetch_add(1, core::sync::atomic::Ordering::Relaxed);

    // Derive a tokened send cap on our bootstrap endpoint. The child uses this
    // as its creator_endpoint; the token lets us identify them on recv.
    let tokened_creator =
        syscall::cap_derive_token(bootstrap_ep, syscall::RIGHTS_SEND, child_token).ok()?;

    let (reply_label, _) = syscall::ipc_call(
        procmgr_ep,
        procmgr_labels::CREATE_PROCESS,
        0,
        &[module_copy, tokened_creator],
    )
    .ok()?;
    if reply_label != 0
    {
        runtime::log!("svcmgr: restart CREATE_PROCESS failed");
        return None;
    }

    // SAFETY: ipc buffer wraps the registered IPC page.
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc.as_ptr()) };
    if cap_count < 2
    {
        runtime::log!("svcmgr: restart reply missing caps");
        return None;
    }

    Some((reply_caps[0], reply_caps[1], child_token))
}

/// Send `START_PROCESS` via the tokened process handle.
fn start_process(process_handle: u32) -> bool
{
    if !matches!(
        syscall::ipc_call(process_handle, procmgr_labels::START_PROCESS, 0, &[]),
        Ok((0, _))
    )
    {
        runtime::log!("svcmgr: restart START_PROCESS failed");
        return false;
    }
    true
}

/// Rebind death notification: create a new event queue on the new thread,
/// remove the old one from the wait set.
fn rebind_death_notification(svc: &mut ServiceEntry, ws_cap: u32, new_thread_cap: u32) -> bool
{
    let _ = syscall::wait_set_remove(ws_cap, svc.event_queue_cap);
    let _ = syscall::cap_delete(svc.event_queue_cap);

    let Ok(new_eq) = syscall::event_queue_create(4)
    else
    {
        runtime::log!("svcmgr: failed to create new event queue for restart");
        return false;
    };

    if syscall::thread_bind_notification(new_thread_cap, new_eq).is_err()
    {
        runtime::log!("svcmgr: failed to rebind death notification");
        return false;
    }

    svc.event_queue_cap = new_eq;
    svc.thread_cap = new_thread_cap;
    true
}
