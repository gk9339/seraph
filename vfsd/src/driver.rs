// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// vfsd/src/driver.rs

//! Filesystem driver process spawning with capability injection.
//!
//! Creates fatfs driver processes via procmgr's two-phase protocol, injects
//! block device, log, procmgr, and service endpoint capabilities, patches
//! `ProcessInfo` with cap descriptors, and starts the process.

use process_abi::{CapDescriptor, CapType};

use crate::VfsdCaps;

/// VA for mapping child `ProcessInfo` frames during cap injection.
const CHILD_PI_VA: u64 = 0x0000_0002_0000_0000; // 8 GiB

/// Maximum cap descriptors when spawning a filesystem driver.
const MAX_DRIVER_DESCS: usize = 8;

/// Spawn the fatfs driver via procmgr with block device and log endpoint caps.
///
/// Returns the driver's IPC endpoint (send cap) on success.
// too_many_lines: two-phase creation is inherently sequential — create, inject
// caps, patch ProcessInfo, and start must happen in order with shared state.
#[allow(clippy::too_many_lines)]
pub fn spawn_fatfs_driver(caps: &VfsdCaps, blk_ep: u32, ipc_buf: *mut u64) -> Option<u32>
{
    // Derive a copy of the fatfs module cap for this spawn. The original
    // is retained so additional fatfs instances can be created for other mounts.
    let module_copy = syscall::cap_derive(caps.fatfs_module_cap, syscall::RIGHTS_ALL).ok()?;

    // Phase 1: CREATE_PROCESS (suspended).
    let (reply_label, _) = syscall::ipc_call(
        caps.procmgr_ep,
        ipc::procmgr_labels::CREATE_PROCESS,
        0,
        &[module_copy],
    )
    .ok()?;
    if reply_label != 0
    {
        runtime::log!("vfsd: fatfs CREATE_PROCESS failed");
        return None;
    }

    // SAFETY: ipc_buf is the registered IPC buffer.
    #[allow(clippy::cast_ptr_alignment)]
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc_buf) };
    if cap_count < 3
    {
        runtime::log!("vfsd: fatfs CREATE_PROCESS reply missing caps");
        return None;
    }
    let process_handle = reply_caps[0];
    let child_cspace = reply_caps[1];
    let pi_frame = reply_caps[2];

    // Create driver endpoint for vfsd-to-driver IPC.
    let driver_ep = syscall::cap_create_endpoint().ok()?;

    // Phase 2: Inject caps.
    let mut descs = [CapDescriptor {
        slot: 0,
        cap_type: CapType::Frame,
        pad: [0; 3],
        aux0: 0,
        aux1: 0,
    }; MAX_DRIVER_DESCS];

    let (desc_count, first_slot) =
        inject_driver_caps(child_cspace, blk_ep, caps, driver_ep, &mut descs);

    // Phase 3: Patch ProcessInfo.
    patch_process_info(pi_frame, caps.self_aspace, &descs, desc_count, first_slot)?;

    // Phase 4: START_PROCESS via tokened process handle.
    start_process(process_handle)?;

    Some(driver_ep)
}

/// Inject block, log, procmgr, and service endpoint caps into a child `CSpace`.
///
/// Returns `(desc_count, first_slot)`.
fn inject_driver_caps(
    child_cspace: u32,
    blk_ep: u32,
    caps: &VfsdCaps,
    driver_ep: u32,
    descs: &mut [CapDescriptor],
) -> (usize, u32)
{
    let mut inj = ipc::CapInjector::new(descs);

    // Inject block device endpoint (send cap).
    ipc::inject_cap(
        blk_ep,
        syscall::RIGHTS_SEND,
        CapType::Frame,
        ipc::BLOCK_ENDPOINT_SENTINEL,
        0,
        child_cspace,
        &mut inj,
    );

    // Inject log endpoint.
    ipc::inject_cap(
        caps.log_ep,
        syscall::RIGHTS_SEND,
        CapType::Frame,
        ipc::LOG_ENDPOINT_SENTINEL,
        0,
        child_cspace,
        &mut inj,
    );

    // Inject procmgr endpoint (for frame allocation).
    ipc::inject_cap(
        caps.procmgr_ep,
        syscall::RIGHTS_SEND_GRANT,
        CapType::Frame,
        0,
        0,
        child_cspace,
        &mut inj,
    );

    // Inject driver service endpoint (receive cap). This is a newly created
    // endpoint — copy directly rather than derive.
    if let Ok(child_slot) = syscall::cap_copy(driver_ep, child_cspace, syscall::RIGHTS_ALL)
    {
        if inj.first_slot == u32::MAX
        {
            inj.first_slot = child_slot;
        }
        if inj.count < inj.descs.len()
        {
            inj.descs[inj.count] = CapDescriptor {
                slot: child_slot,
                cap_type: CapType::Frame,
                pad: [0; 3],
                aux0: ipc::SERVICE_ENDPOINT_SENTINEL,
                aux1: 0,
            };
            inj.count += 1;
        }
    }

    let count = inj.count;
    let first = inj.first_slot;
    (count, first)
}

/// Map `ProcessInfo`, write cap descriptors, then unmap.
fn patch_process_info(
    pi_frame: u32,
    self_aspace: u32,
    descs: &[CapDescriptor],
    desc_count: usize,
    first_slot: u32,
) -> Option<()>
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
        runtime::log!("vfsd: cannot map fatfs ProcessInfo");
        return None;
    }

    // SAFETY: CHILD_PI_VA is mapped writable to the ProcessInfo page.
    unsafe { ipc::write_cap_descriptors(CHILD_PI_VA, descs, desc_count, first_slot) };

    let _ = syscall::mem_unmap(self_aspace, CHILD_PI_VA, 1);
    Some(())
}

/// Start a previously created process via its tokened handle.
fn start_process(process_handle: u32) -> Option<()>
{
    if let Ok((0, _)) =
        syscall::ipc_call(process_handle, ipc::procmgr_labels::START_PROCESS, 0, &[])
    {
        runtime::log!("vfsd: fatfs driver started");
        Some(())
    }
    else
    {
        runtime::log!("vfsd: fatfs START_PROCESS failed");
        None
    }
}
