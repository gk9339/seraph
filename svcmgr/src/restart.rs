// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// svcmgr/src/restart.rs

//! Service death handling and restart logic.
//!
//! Detects whether a crashed service should be restarted based on its restart
//! policy and criticality, then creates a new process instance via procmgr,
//! injects capabilities, and rebinds death notification.

use crate::halt_loop;
use crate::service::{
    ServiceEntry, CHILD_PI_VA, CRITICALITY_FATAL, CRITICALITY_NORMAL, MAX_RESTARTS, POLICY_ALWAYS,
    POLICY_ON_FAILURE,
};
use ipc::{inject_cap, procmgr_labels, write_cap_descriptors, CapInjector, LOG_ENDPOINT_SENTINEL};
use process_abi::{CapDescriptor, CapType};

/// Handle a service death detected via event queue notification.
///
/// Checks criticality and restart policy, then attempts to restart the service
/// if appropriate. Marks the service inactive if restart is not attempted or
/// fails.
pub fn handle_death(
    svc: &mut ServiceEntry,
    exit_reason: u64,
    procmgr_ep: u32,
    self_aspace: u32,
    log_ep: u32,
    ws_cap: u32,
    ipc_buf: *mut u64,
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

    if !restart_process(svc, procmgr_ep, self_aspace, log_ep, ws_cap, ipc_buf)
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
        _ => false, // POLICY_NEVER or unknown
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

/// Create a new process instance via procmgr, inject caps, start it, and
/// rebind death notification. Returns `true` on success.
#[allow(clippy::too_many_lines)]
fn restart_process(
    svc: &mut ServiceEntry,
    procmgr_ep: u32,
    self_aspace: u32,
    log_ep: u32,
    ws_cap: u32,
    ipc_buf: *mut u64,
) -> bool
{
    let Some((process_handle, child_cspace, pi_frame, new_thread_cap)) =
        create_process(svc, procmgr_ep, ipc_buf)
    else
    {
        return false;
    };

    // Inject log endpoint into child CSpace.
    let inject_log = if svc.log_ep_cap != 0
    {
        svc.log_ep_cap
    }
    else
    {
        log_ep
    };

    let mut desc_buf = [CapDescriptor {
        slot: 0,
        cap_type: CapType::Frame,
        pad: [0; 3],
        aux0: 0,
        aux1: 0,
    }; 4];
    let mut inj = CapInjector::new(&mut desc_buf);

    if inject_log != 0
    {
        inject_cap(
            inject_log,
            syscall::RIGHTS_SEND,
            CapType::Frame,
            LOG_ENDPOINT_SENTINEL,
            0,
            child_cspace,
            &mut inj,
        );
    }

    // Extract count and first_slot before immutable borrow of desc_buf.
    let count = inj.count;
    let first_slot = inj.first_slot;

    // Write capability descriptors into the child's ProcessInfo page.
    if count > 0
        && !map_and_write_descriptors(
            pi_frame,
            self_aspace,
            &inj.descs[..count],
            count,
            first_slot,
        )
    {
        return false;
    }

    // Start the new process via the tokened process handle.
    if !start_process(process_handle)
    {
        return false;
    }

    // Rebind death notification to the new thread.
    rebind_death_notification(svc, ws_cap, new_thread_cap)
}

/// Send `CREATE_PROCESS` to procmgr and extract reply caps.
///
/// Returns `(process_handle, child_cspace, pi_frame, new_thread_cap)` on success.
fn create_process(
    svc: &ServiceEntry,
    procmgr_ep: u32,
    ipc_buf: *mut u64,
) -> Option<(u32, u32, u32, u32)>
{
    let module_copy = syscall::cap_derive(svc.module_cap, syscall::RIGHTS_ALL).ok()?;

    let (reply_label, _) = syscall::ipc_call(
        procmgr_ep,
        procmgr_labels::CREATE_PROCESS,
        0,
        &[module_copy],
    )
    .ok()?;
    if reply_label != 0
    {
        runtime::log!("svcmgr: restart CREATE_PROCESS failed");
        return None;
    }

    // SAFETY: ipc_buf is the registered IPC buffer, page-aligned.
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc_buf.cast::<u64>()) };
    if cap_count < 4
    {
        runtime::log!("svcmgr: restart reply missing caps");
        return None;
    }

    Some((reply_caps[0], reply_caps[1], reply_caps[2], reply_caps[3]))
}

/// Map the child `ProcessInfo` frame, write descriptors, and unmap.
fn map_and_write_descriptors(
    pi_frame: u32,
    self_aspace: u32,
    descs: &[CapDescriptor],
    count: usize,
    first_slot: u32,
) -> bool
{
    if syscall::mem_map(
        pi_frame,
        self_aspace,
        CHILD_PI_VA,
        0,
        1,
        syscall::MAP_WRITABLE,
    )
    .is_err()
    {
        runtime::log!("svcmgr: cannot map child ProcessInfo");
        return false;
    }

    // SAFETY: CHILD_PI_VA is mapped writable to the ProcessInfo page and is
    // page-aligned (4096-byte). The page remains mapped for this call.
    unsafe { write_cap_descriptors(CHILD_PI_VA, descs, count, first_slot) };

    let _ = syscall::mem_unmap(self_aspace, CHILD_PI_VA, 1);
    true
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
