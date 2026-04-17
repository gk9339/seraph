// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// vfsd/src/driver.rs

//! Filesystem driver process spawning.
//!
//! Creates fatfs driver processes via procmgr's two-phase protocol. The child
//! is spawned with a tokened SEND cap on vfsd's worker-owned bootstrap
//! endpoint as its creator endpoint. Main publishes a plan keyed by that
//! token, then blocks on `done_sig` while the worker thread delivers the
//! bootstrap round. After the worker signals completion, main sends a
//! zero-payload `FS_MOUNT` to the driver as a BPB-validation probe.
//!
//! This routes fatfs through the generic bootstrap protocol without
//! clobbering the main thread's reply target (= init) while servicing MOUNT.

use ipc::{fs_labels, procmgr_labels, IpcBuf};

use crate::{worker, VfsdCaps};

/// Monotonic counter for fatfs-child bootstrap tokens.
static NEXT_BOOTSTRAP_TOKEN: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(1);

/// Spawn the fatfs driver via procmgr, deliver its cap set over the bootstrap
/// protocol, and probe it with `FS_MOUNT` to confirm BPB validation.
///
/// Returns the driver's IPC endpoint (send cap) on success.
pub fn spawn_fatfs_driver(
    caps: &VfsdCaps,
    blk_ep: u32,
    partition_lba: u64,
    ipc: IpcBuf,
) -> Option<u32>
{
    if caps.bootstrap_ep == 0 || caps.done_sig == 0
    {
        runtime::log!("vfsd: spawn_fatfs: worker thread not initialised");
        return None;
    }

    let module_copy = syscall::cap_derive(caps.fatfs_module_cap, syscall::RIGHTS_ALL).ok()?;

    // Create fatfs's service endpoint. fatfs receives service calls on this;
    // vfsd holds a SEND_GRANT copy for forwarding FS_OPEN.
    let driver_ep = syscall::cap_create_endpoint().ok()?;
    let driver_ep_for_child = syscall::cap_derive(driver_ep, syscall::RIGHTS_ALL).ok()?;
    let driver_send = syscall::cap_derive(driver_ep, syscall::RIGHTS_SEND_GRANT).ok()?;

    // Derive the caps delivered to fatfs via bootstrap.
    let blk_copy = syscall::cap_derive(blk_ep, syscall::RIGHTS_SEND).ok()?;
    let log_copy = if caps.log_ep != 0
    {
        syscall::cap_derive(caps.log_ep, syscall::RIGHTS_SEND).unwrap_or(0)
    }
    else
    {
        0
    };

    // Allocate a bootstrap token and publish the plan for the worker.
    let token = NEXT_BOOTSTRAP_TOKEN.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    let tokened_creator =
        syscall::cap_derive_token(caps.bootstrap_ep, syscall::RIGHTS_SEND, token).ok()?;
    worker::publish_plan(
        token,
        blk_copy,
        log_copy,
        driver_ep_for_child,
        partition_lba,
    );

    // Phase 1: CREATE_PROCESS with [module, tokened bootstrap cap].
    let (reply_label, _) = syscall::ipc_call(
        caps.procmgr_ep,
        procmgr_labels::CREATE_PROCESS,
        0,
        &[module_copy, tokened_creator],
    )
    .ok()?;
    if reply_label != 0
    {
        runtime::log!("vfsd: fatfs CREATE_PROCESS failed");
        return None;
    }

    // SAFETY: ipc wraps the registered IPC buffer.
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc.as_ptr()) };
    if cap_count < 2
    {
        runtime::log!("vfsd: fatfs CREATE_PROCESS reply missing caps");
        return None;
    }
    let process_handle = reply_caps[0];

    // START_PROCESS — fatfs begins executing and issues its bootstrap request.
    if !matches!(
        syscall::ipc_call(process_handle, procmgr_labels::START_PROCESS, 0, &[]),
        Ok((0, _))
    )
    {
        runtime::log!("vfsd: fatfs START_PROCESS failed");
        return None;
    }

    // Wait for the worker to deliver the bootstrap round.
    let Ok(bits) = syscall::signal_wait(caps.done_sig)
    else
    {
        runtime::log!("vfsd: fatfs bootstrap signal_wait failed");
        return None;
    };
    if bits != 1
    {
        runtime::log!("vfsd: fatfs bootstrap delivery failed (bits={:#x})", bits);
        return None;
    }

    // Probe the driver with an empty FS_MOUNT: fatfs validates the BPB in its
    // handler and replies with fs_errors::SUCCESS or an error label.
    let (mount_reply, _) = syscall::ipc_call(driver_send, fs_labels::FS_MOUNT, 0, &[]).ok()?;
    if mount_reply != 0
    {
        runtime::log!("vfsd: fatfs FS_MOUNT probe failed (label={})", mount_reply);
        return None;
    }

    runtime::log!("vfsd: fatfs driver started");
    Some(driver_send)
}
