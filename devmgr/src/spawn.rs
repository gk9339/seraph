// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// devmgr/src/spawn.rs

//! Driver process spawning with per-device capability injection.
//!
//! Creates driver processes via procmgr, injects MMIO, IRQ, and service
//! endpoint capabilities, patches `ProcessInfo` with cap descriptors and
//! a `VirtIO` startup message, then starts the process.

use process_abi::{CapDescriptor, CapType, ProcessInfo};
use virtio_core::VirtioPciStartupInfo;

const PAGE_SIZE: usize = 0x1000;

/// VA for mapping driver `ProcessInfo` frames during cap injection.
const DRIVER_PI_VA: u64 = 0x0000_0002_0000_0000; // 8 GiB

/// Maximum cap descriptors for driver delegation.
const MAX_DRIVER_DESCS: usize = 16;

/// Spawn a driver process with per-device capabilities.
///
/// Requests procmgr to create the process, injects MMIO and IRQ caps, patches
/// `ProcessInfo`, and starts the process.
// too_many_arguments: driver spawning requires per-device BAR caps, IRQ,
// procmgr endpoint, and startup message; grouping would add complexity
// without improving clarity.
#[allow(clippy::too_many_arguments)]
pub fn spawn_driver(
    procmgr_ep: u32,
    module_cap: u32,
    self_aspace: u32,
    bar_caps: &[u32],
    bar_bases: &[u64],
    bar_sizes: &[u64],
    irq_cap: Option<u32>,
    irq_id: u32,
    log_ep: u32,
    service_ep: u32,
    virtio_info: &VirtioPciStartupInfo,
    ipc_buf: *mut u64,
)
{
    // Phase 1: CREATE_PROCESS via procmgr.
    let Ok((reply_label, _)) = syscall::ipc_call(
        procmgr_ep,
        ipc::procmgr_labels::CREATE_PROCESS,
        0,
        &[module_cap],
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

    // SAFETY: ipc_buf is the registered IPC buffer.
    #[allow(clippy::cast_ptr_alignment)]
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc_buf) };
    if cap_count < 3
    {
        runtime::log!("devmgr: driver CREATE_PROCESS reply missing caps");
        return;
    }
    let process_handle = reply_caps[0];
    let child_cspace = reply_caps[1];
    let pi_frame = reply_caps[2];

    // Phase 2: Inject per-device caps.
    let mut descs = [CapDescriptor {
        slot: 0,
        cap_type: CapType::Frame,
        pad: [0; 3],
        aux0: 0,
        aux1: 0,
    }; MAX_DRIVER_DESCS];

    let (desc_count, first_slot) = inject_driver_caps(
        child_cspace,
        bar_caps,
        bar_bases,
        bar_sizes,
        irq_cap,
        irq_id,
        procmgr_ep,
        log_ep,
        service_ep,
        &mut descs,
    );

    // Phase 3: Patch ProcessInfo with cap descriptors and startup message.
    patch_process_info(
        pi_frame,
        self_aspace,
        &descs,
        desc_count,
        first_slot,
        virtio_info,
    );

    // Phase 4: START_PROCESS via tokened process handle.
    match syscall::ipc_call(process_handle, ipc::procmgr_labels::START_PROCESS, 0, &[])
    {
        Ok((0, _)) => runtime::log!("devmgr: driver started"),
        _ => runtime::log!("devmgr: driver START_PROCESS failed"),
    }
}

/// Inject BAR, IRQ, endpoint, and log caps into a child `CSpace`.
///
/// Returns `(desc_count, first_slot)`.
// too_many_arguments: accepts all per-device cap sources; grouping would not
// reduce total parameter surface.
#[allow(clippy::too_many_arguments)]
fn inject_driver_caps(
    child_cspace: u32,
    bar_caps: &[u32],
    bar_bases: &[u64],
    bar_sizes: &[u64],
    irq_cap: Option<u32>,
    irq_id: u32,
    procmgr_ep: u32,
    log_ep: u32,
    service_ep: u32,
    descs: &mut [CapDescriptor],
) -> (usize, u32)
{
    let mut inj = ipc::CapInjector::new(descs);

    for (i, &bar_cap) in bar_caps.iter().enumerate()
    {
        ipc::inject_cap(
            bar_cap,
            syscall::RIGHTS_ALL,
            CapType::MmioRegion,
            bar_bases[i],
            bar_sizes[i],
            child_cspace,
            &mut inj,
        );
    }
    if let Some(irq_slot) = irq_cap
    {
        ipc::inject_cap(
            irq_slot,
            syscall::RIGHTS_ALL,
            CapType::Interrupt,
            u64::from(irq_id),
            0,
            child_cspace,
            &mut inj,
        );
    }
    ipc::inject_cap(
        procmgr_ep,
        syscall::RIGHTS_SEND_GRANT,
        CapType::Frame,
        0,
        0,
        child_cspace,
        &mut inj,
    );
    if log_ep != 0
    {
        ipc::inject_cap(
            log_ep,
            syscall::RIGHTS_SEND,
            CapType::Frame,
            ipc::LOG_ENDPOINT_SENTINEL,
            0,
            child_cspace,
            &mut inj,
        );
    }
    if service_ep != 0
    {
        ipc::inject_cap(
            service_ep,
            syscall::RIGHTS_ALL,
            CapType::Frame,
            ipc::SERVICE_ENDPOINT_SENTINEL,
            0,
            child_cspace,
            &mut inj,
        );
    }

    (inj.count, inj.first_slot)
}

/// Map `ProcessInfo`, write cap descriptors and startup message, then unmap.
fn patch_process_info(
    pi_frame: u32,
    self_aspace: u32,
    descs: &[CapDescriptor],
    desc_count: usize,
    first_slot: u32,
    virtio_info: &VirtioPciStartupInfo,
)
{
    if syscall::mem_map(
        pi_frame,
        self_aspace,
        DRIVER_PI_VA,
        0,
        1,
        syscall::MAP_WRITABLE,
    )
    .is_err()
    {
        runtime::log!("devmgr: cannot map driver ProcessInfo");
        return;
    }

    // SAFETY: DRIVER_PI_VA is mapped writable to the ProcessInfo page.
    unsafe { ipc::write_cap_descriptors(DRIVER_PI_VA, descs, desc_count, first_slot) };
    write_startup_message(DRIVER_PI_VA, desc_count, virtio_info);

    let _ = syscall::mem_unmap(self_aspace, DRIVER_PI_VA, 1);
}

/// Write `VirtIO` PCI startup message into `ProcessInfo` after cap descriptors.
fn write_startup_message(pi_va: u64, desc_count: usize, virtio_info: &VirtioPciStartupInfo)
{
    let descs_start = (core::mem::size_of::<ProcessInfo>() + 7) & !7;
    let msg_offset = descs_start + desc_count * core::mem::size_of::<CapDescriptor>();
    let msg_offset_aligned = (msg_offset + 7) & !7;

    if msg_offset_aligned + VirtioPciStartupInfo::SIZE > PAGE_SIZE
    {
        return;
    }

    // SAFETY: pi_va is mapped writable; byte range is within the page.
    let msg_buf = unsafe {
        core::slice::from_raw_parts_mut(
            (pi_va as *mut u8).add(msg_offset_aligned),
            VirtioPciStartupInfo::SIZE,
        )
    };
    let _ = virtio_info.to_bytes(msg_buf);

    // SAFETY: pi_va is page-aligned and mapped writable.
    #[allow(clippy::cast_ptr_alignment)]
    let pi = unsafe { &mut *(pi_va as *mut ProcessInfo) };
    pi.startup_message_offset = msg_offset_aligned as u32;
    pi.startup_message_len = VirtioPciStartupInfo::SIZE as u32;
}
