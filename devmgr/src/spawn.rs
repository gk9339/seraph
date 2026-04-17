// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// devmgr/src/spawn.rs

//! Driver process spawning with per-device capability delivery via bootstrap.
//!
//! Creates driver processes via procmgr, then serves the driver's bootstrap
//! over IPC to deliver its per-device capability set (BAR MMIO, IRQ, service
//! endpoint, log, procmgr endpoint, devmgr query endpoint).

use ipc::{procmgr_labels, IpcBuf};

/// Monotonic counter for driver-child bootstrap tokens.
static NEXT_BOOTSTRAP_TOKEN: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(1);

/// Spawn a driver process with per-device capabilities.
///
/// Creates the process via procmgr, starts it, and serves its bootstrap over
/// IPC to deliver the BAR MMIO, IRQ, and endpoint caps. The `device_token` is
/// used to derive a per-device tokened send cap from `registry_ep` so the
/// driver can query devmgr for its device configuration.
///
/// Layout matches `drivers/virtio/blk/src/main.rs::bootstrap_caps`:
///   Round 1 (4 caps): BAR MMIO, IRQ, driver service endpoint, log endpoint.
///   Round 2 (2 caps): procmgr endpoint, devmgr query endpoint.
// too_many_arguments: driver spawning requires per-device BAR caps, IRQ,
// procmgr endpoint, and registry endpoint.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn spawn_driver(
    procmgr_ep: u32,
    bootstrap_ep: u32,
    module_cap: u32,
    bar_caps: &[u32],
    bar_bases: &[u64],
    bar_sizes: &[u64],
    irq_cap: Option<u32>,
    _irq_id: u32,
    log_ep: u32,
    service_ep: u32,
    registry_ep: u32,
    device_token: u64,
    ipc: IpcBuf,
)
{
    let _ = bar_bases;
    let _ = bar_sizes;

    let Some(bar_cap) = bar_caps.first().copied()
    else
    {
        runtime::log!("devmgr: driver spawn: no BAR cap");
        return;
    };
    let Some(irq_slot) = irq_cap
    else
    {
        runtime::log!("devmgr: driver spawn: no IRQ cap");
        return;
    };

    // Allocate a bootstrap token for the child.
    let child_token = NEXT_BOOTSTRAP_TOKEN.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    let Ok(tokened_creator) =
        syscall::cap_derive_token(bootstrap_ep, syscall::RIGHTS_SEND, child_token)
    else
    {
        runtime::log!("devmgr: driver spawn: tokened creator derivation failed");
        return;
    };

    // Phase 1: CREATE_PROCESS via procmgr.
    let Ok((reply_label, _)) = syscall::ipc_call(
        procmgr_ep,
        procmgr_labels::CREATE_PROCESS,
        0,
        &[module_cap, tokened_creator],
    )
    else
    {
        runtime::log!("devmgr: driver CREATE_PROCESS ipc_call failed");
        return;
    };
    if reply_label != 0
    {
        runtime::log!("devmgr: driver CREATE_PROCESS failed");
        return;
    }

    // SAFETY: ipc is registered IPC buffer.
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc.as_ptr()) };
    if cap_count < 2
    {
        runtime::log!("devmgr: driver CREATE_PROCESS reply missing caps");
        return;
    }
    let process_handle = reply_caps[0];

    // Derive all per-child caps for delivery via bootstrap.
    let Ok(bar_copy) = syscall::cap_derive(bar_cap, syscall::RIGHTS_ALL)
    else
    {
        return;
    };
    let Ok(irq_copy) = syscall::cap_derive(irq_slot, syscall::RIGHTS_ALL)
    else
    {
        return;
    };
    let Ok(procmgr_copy) = syscall::cap_derive(procmgr_ep, syscall::RIGHTS_SEND_GRANT)
    else
    {
        return;
    };
    let log_copy = if log_ep != 0
    {
        syscall::cap_derive(log_ep, syscall::RIGHTS_SEND).unwrap_or(0)
    }
    else
    {
        0
    };
    let service_copy = if service_ep != 0
    {
        syscall::cap_derive(service_ep, syscall::RIGHTS_ALL).unwrap_or(0)
    }
    else
    {
        0
    };
    let Ok(devmgr_copy) =
        syscall::cap_derive_token(registry_ep, syscall::RIGHTS_SEND, device_token)
    else
    {
        return;
    };

    // START_PROCESS.
    if !matches!(
        syscall::ipc_call(process_handle, procmgr_labels::START_PROCESS, 0, &[]),
        Ok((0, _))
    )
    {
        runtime::log!("devmgr: driver START_PROCESS failed");
        return;
    }

    // Serve bootstrap round 1: [bar, irq, service, log].
    if ipc::bootstrap::serve_round(
        bootstrap_ep,
        child_token,
        ipc,
        false,
        &[bar_copy, irq_copy, service_copy, log_copy],
        &[],
    )
    .is_err()
    {
        runtime::log!("devmgr: driver bootstrap round 1 failed");
        return;
    }

    // Round 2: [procmgr, devmgr_query], done.
    if ipc::bootstrap::serve_round(
        bootstrap_ep,
        child_token,
        ipc,
        true,
        &[procmgr_copy, devmgr_copy],
        &[],
    )
    .is_err()
    {
        runtime::log!("devmgr: driver bootstrap round 2 failed");
        return;
    }

    runtime::log!("devmgr: driver started");
}
