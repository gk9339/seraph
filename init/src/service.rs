// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// init/src/service.rs

//! Service creation helpers for init.
//!
//! Provides two-phase process creation through procmgr IPC: create suspended,
//! inject capabilities into the child `CSpace`, patch `ProcessInfo` with
//! `CapDescriptor` entries, then start. Uses the shared `ipc` crate for cap
//! injection and `ProcessInfo` descriptor writing.

use crate::logging::log;
use crate::{idle_loop, TEMP_MAP_BASE};
use init_protocol::{CapDescriptor, CapType, InitInfo};
use ipc::{
    inject_cap, procmgr_labels, svcmgr_labels, write_path_to_ipc, CapInjector,
    LOG_ENDPOINT_SENTINEL, PROCMGR_ENDPOINT_SENTINEL, REGISTRY_ENDPOINT_SENTINEL,
    SERVICE_ENDPOINT_SENTINEL,
};

// ── Constants ────────────────────────────────────────────────────────────────

/// Temp VA for mapping child `ProcessInfo` frames during cap descriptor patching.
const CHILD_PI_TEMP_VA: u64 = TEMP_MAP_BASE + 0x2000_0000;

/// Max cap descriptors for devmgr delegation.
const DEVMGR_MAX_DESCS: usize = 128;

/// Max cap descriptors for vfsd delegation.
const VFSD_MAX_DESCS: usize = 16;

// ── Cap injection + ProcessInfo patching ─────────────────────────────────────

/// Map a child's `ProcessInfo` frame, write cap descriptors, then unmap.
fn patch_process_info(
    aspace_cap: u32,
    pi_frame: u32,
    desc_buf: &[CapDescriptor],
    desc_count: usize,
    first_slot: u32,
    context: &str,
) -> bool
{
    if syscall::mem_map(
        pi_frame,
        aspace_cap,
        CHILD_PI_TEMP_VA,
        0,
        1,
        syscall::MAP_WRITABLE,
    )
    .is_err()
    {
        log(context);
        return false;
    }

    // SAFETY: CHILD_PI_TEMP_VA is mapped writable to the ProcessInfo page.
    unsafe { ipc::write_cap_descriptors(CHILD_PI_TEMP_VA, desc_buf, desc_count, first_slot) };

    let _ = syscall::mem_unmap(aspace_cap, CHILD_PI_TEMP_VA, 1);
    true
}

/// Start a process by calling `START_PROCESS` on its tokened process handle.
fn start_process(process_handle: u32, ok_msg: &str, fail_msg: &str)
{
    match syscall::ipc_call(process_handle, procmgr_labels::START_PROCESS, 0, &[])
    {
        Ok((0, _)) => log(ok_msg),
        _ => log(fail_msg),
    }
}

// ── devmgr creation ──────────────────────────────────────────────────────────

/// Create devmgr with full hardware cap delegation.
///
/// Two-phase process creation: `CREATE_PROCESS` (suspended), inject hardware
/// caps and driver module caps, patch `ProcessInfo` with `CapDescriptor`
/// entries, then `START_PROCESS`.
// too_many_lines: hardware cap delegation is inherently sequential with many
// cap operations; splitting would fragment the delegation flow.
#[allow(clippy::too_many_lines)]
pub fn create_devmgr_with_caps(
    info: &InitInfo,
    procmgr_ep: u32,
    log_ep: u32,
    registry_ep: u32,
    ipc_buf: *mut u64,
)
{
    let devmgr_frame_cap = info.module_frame_base + 1; // Module 1 = devmgr

    // Phase 1: CREATE_PROCESS (suspended).
    let Ok((reply_label, _)) = syscall::ipc_call(
        procmgr_ep,
        procmgr_labels::CREATE_PROCESS,
        0,
        &[devmgr_frame_cap],
    )
    else
    {
        log("init: devmgr: CREATE_PROCESS ipc_call failed");
        return;
    };
    if reply_label != 0
    {
        log("init: devmgr: CREATE_PROCESS failed");
        return;
    }

    // Read process handle, child CSpace, and ProcessInfo frame from reply caps.
    // SAFETY: ipc_buf is the registered IPC buffer.
    #[allow(clippy::cast_ptr_alignment)]
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc_buf) };
    if cap_count < 3
    {
        log("init: devmgr: CREATE_PROCESS reply missing caps");
        return;
    }
    let process_handle = reply_caps[0];
    let child_cspace = reply_caps[1];
    let pi_frame = reply_caps[2];

    // Phase 2: Inject hardware caps.
    let mut desc_buf = [CapDescriptor {
        slot: 0,
        cap_type: CapType::Frame,
        pad: [0; 3],
        aux0: 0,
        aux1: 0,
    }; DEVMGR_MAX_DESCS];
    let mut inj = CapInjector::new(&mut desc_buf);

    // Inject all hardware caps from init's cap descriptor table.
    let init_descs = crate::descriptors(info);
    for d in init_descs
    {
        match d.cap_type
        {
            CapType::MmioRegion
            | CapType::PciEcam
            | CapType::Interrupt
            | CapType::IoPortRange
            | CapType::SchedControl =>
            {
                inject_cap(
                    d.slot,
                    syscall::RIGHTS_ALL,
                    d.cap_type,
                    d.aux0,
                    d.aux1,
                    child_cspace,
                    &mut inj,
                );
            }
            // Skip memory frames and SBI control — devmgr doesn't need them.
            _ =>
            {}
        }
    }

    // Inject procmgr endpoint cap so devmgr can spawn drivers and request frames.
    inject_cap(
        procmgr_ep,
        syscall::RIGHTS_SEND_GRANT,
        CapType::Frame,
        0,
        0,
        child_cspace,
        &mut inj,
    );

    // Inject driver module frame cap (module 3 = virtio-blk only; module 4+
    // are filesystem drivers delegated to vfsd instead).
    if info.module_frame_count > 3
    {
        let module_cap = info.module_frame_base + 3;
        inject_cap(
            module_cap,
            syscall::RIGHTS_ALL,
            CapType::Frame,
            3, // aux0 = module index
            0,
            child_cspace,
            &mut inj,
        );
    }

    // Inject log endpoint cap (sentinel: Frame with aux0=LOG_ENDPOINT_SENTINEL).
    inject_cap(
        log_ep,
        syscall::RIGHTS_SEND,
        CapType::Frame,
        LOG_ENDPOINT_SENTINEL,
        0,
        child_cspace,
        &mut inj,
    );

    // Inject device registry endpoint (devmgr receives on this).
    inject_cap(
        registry_ep,
        syscall::RIGHTS_ALL,
        CapType::Frame,
        REGISTRY_ENDPOINT_SENTINEL,
        0,
        child_cspace,
        &mut inj,
    );

    // Extract count/first_slot before borrowing desc_buf through inj.
    let count = inj.count;
    let first_slot = inj.first_slot;

    // Phase 3: Patch ProcessInfo page with CapDescriptors.
    if !patch_process_info(
        info.aspace_cap,
        pi_frame,
        &desc_buf,
        count,
        first_slot,
        "init: devmgr: cannot map ProcessInfo frame",
    )
    {
        return;
    }

    // Phase 4: START_PROCESS.
    start_process(
        process_handle,
        "init: devmgr created and started with hardware caps",
        "init: devmgr: START_PROCESS failed",
    );
}

// ── vfsd creation ────────────────────────────────────────────────────────────

/// Create vfsd with caps needed for filesystem support.
///
/// Two-phase creation: `CREATE_PROCESS` (suspended), inject caps (log,
/// procmgr, devmgr registry, fatfs module, service endpoint), then
/// `START_PROCESS`.
// too_many_lines: cap delegation is inherently sequential; splitting would
// fragment the delegation flow.
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub fn create_vfsd_with_caps(
    info: &InitInfo,
    procmgr_ep: u32,
    log_ep: u32,
    registry_ep: u32,
    vfsd_service_ep: u32,
    ipc_buf: *mut u64,
)
{
    let vfsd_frame_cap = info.module_frame_base + 2; // Module 2 = vfsd

    // Phase 1: CREATE_PROCESS (suspended).
    let Ok((reply_label, _)) = syscall::ipc_call(
        procmgr_ep,
        procmgr_labels::CREATE_PROCESS,
        0,
        &[vfsd_frame_cap],
    )
    else
    {
        log("init: vfsd: CREATE_PROCESS ipc_call failed");
        return;
    };
    if reply_label != 0
    {
        log("init: vfsd: CREATE_PROCESS failed");
        return;
    }

    // SAFETY: ipc_buf is the registered IPC buffer.
    #[allow(clippy::cast_ptr_alignment)]
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc_buf) };
    if cap_count < 3
    {
        log("init: vfsd: CREATE_PROCESS reply missing caps");
        return;
    }
    let process_handle = reply_caps[0];
    let child_cspace = reply_caps[1];
    let pi_frame = reply_caps[2];

    // Phase 2: Inject caps.
    let mut desc_buf = [CapDescriptor {
        slot: 0,
        cap_type: CapType::Frame,
        pad: [0; 3],
        aux0: 0,
        aux1: 0,
    }; VFSD_MAX_DESCS];
    let mut inj = CapInjector::new(&mut desc_buf);

    // Log endpoint.
    inject_cap(
        log_ep,
        syscall::RIGHTS_SEND,
        CapType::Frame,
        LOG_ENDPOINT_SENTINEL,
        0,
        child_cspace,
        &mut inj,
    );

    // procmgr endpoint (sentinel: aux0=0, aux1=0).
    inject_cap(
        procmgr_ep,
        syscall::RIGHTS_SEND_GRANT,
        CapType::Frame,
        0,
        0,
        child_cspace,
        &mut inj,
    );

    // devmgr registry endpoint (send cap — vfsd queries devmgr).
    inject_cap(
        registry_ep,
        syscall::RIGHTS_SEND,
        CapType::Frame,
        REGISTRY_ENDPOINT_SENTINEL,
        0,
        child_cspace,
        &mut inj,
    );

    // fatfs module frame cap (module 4).
    if info.module_frame_count > 4
    {
        let fatfs_cap = info.module_frame_base + 4;
        inject_cap(
            fatfs_cap,
            syscall::RIGHTS_ALL,
            CapType::Frame,
            4, // aux0 = module index
            0,
            child_cspace,
            &mut inj,
        );
    }

    // vfsd service endpoint (vfsd receives on this).
    inject_cap(
        vfsd_service_ep,
        syscall::RIGHTS_ALL,
        CapType::Frame,
        SERVICE_ENDPOINT_SENTINEL,
        0,
        child_cspace,
        &mut inj,
    );

    // Extract count/first_slot before borrowing desc_buf through inj.
    let count = inj.count;
    let first_slot = inj.first_slot;

    // Phase 3: Patch ProcessInfo page.
    if !patch_process_info(
        info.aspace_cap,
        pi_frame,
        &desc_buf,
        count,
        first_slot,
        "init: vfsd: cannot map ProcessInfo frame",
    )
    {
        return;
    }

    // Phase 4: START_PROCESS.
    start_process(
        process_handle,
        "init: vfsd created and started with caps",
        "init: vfsd: START_PROCESS failed",
    );
}

// ── Generic service creation ─────────────────────────────────────────────────

/// Create a service via procmgr with a log endpoint cap injected.
///
/// Two-phase creation: `CREATE_PROCESS` (suspended), inject log endpoint cap
/// into child `CSpace`, patch `ProcessInfo` with `CapDescriptor`, then
/// `START_PROCESS`.
#[allow(clippy::too_many_arguments, dead_code)]
pub fn create_and_start_service_with_log(
    info: &InitInfo,
    procmgr_ep: u32,
    module_frame_cap: u32,
    log_ep: u32,
    ipc_buf: *mut u64,
    ok_msg: &str,
    fail_msg: &str,
)
{
    // Phase 1: CREATE_PROCESS (suspended).
    let Ok((reply_label, _)) = syscall::ipc_call(
        procmgr_ep,
        procmgr_labels::CREATE_PROCESS,
        0,
        &[module_frame_cap],
    )
    else
    {
        log(fail_msg);
        return;
    };
    if reply_label != 0
    {
        log(fail_msg);
        return;
    }

    // Read process handle and child caps.
    // SAFETY: ipc_buf is the registered IPC buffer.
    #[allow(clippy::cast_ptr_alignment)]
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc_buf) };
    if cap_count < 3
    {
        log(fail_msg);
        return;
    }
    let process_handle = reply_caps[0];
    let child_cspace = reply_caps[1];
    let pi_frame = reply_caps[2];

    // Phase 2: Inject log endpoint cap.
    let mut desc_buf = [CapDescriptor {
        slot: 0,
        cap_type: CapType::Frame,
        pad: [0; 3],
        aux0: 0,
        aux1: 0,
    }; 4];
    let mut inj = CapInjector::new(&mut desc_buf);

    inject_cap(
        log_ep,
        syscall::RIGHTS_SEND,
        CapType::Frame,
        LOG_ENDPOINT_SENTINEL,
        0,
        child_cspace,
        &mut inj,
    );

    let count = inj.count;
    let first_slot = inj.first_slot;

    // Phase 3: Patch ProcessInfo.
    if !patch_process_info(
        info.aspace_cap,
        pi_frame,
        &desc_buf,
        count,
        first_slot,
        fail_msg,
    )
    {
        return;
    }

    // Phase 4: START_PROCESS.
    start_process(process_handle, ok_msg, fail_msg);
}

// ── svcmgr / procmgr coordination ───────────────────────────────────────────

/// Send `SET_VFSD_ENDPOINT` to procmgr so it can do VFS-based ELF loading.
pub fn send_vfsd_endpoint_to_procmgr(procmgr_ep: u32, vfsd_ep: u32)
{
    let Ok(vfsd_copy) = syscall::cap_derive(vfsd_ep, syscall::RIGHTS_SEND_GRANT)
    else
    {
        log("init: phase 3: failed to derive vfsd endpoint");
        return;
    };
    match syscall::ipc_call(procmgr_ep, procmgr_labels::SET_VFSD_EP, 0, &[vfsd_copy])
    {
        Ok((0, _)) => log("init: phase 3: vfsd endpoint sent to procmgr"),
        _ => log("init: phase 3: SET_VFSD_ENDPOINT failed"),
    }
}

/// Create svcmgr from VFS (`/bin/svcmgr`) via `CREATE_PROCESS_FROM_VFS`.
///
/// Returns `(process_handle, child_cspace, pi_frame, thread_cap)` on success.
pub fn create_svcmgr_from_vfs(procmgr_ep: u32, ipc_buf: *mut u64) -> Option<(u32, u32, u32, u32)>
{
    let path = b"/bin/svcmgr";

    // Pack path into IPC buffer data words.
    // SAFETY: ipc_buf is the registered IPC buffer.
    let word_count = unsafe { write_path_to_ipc(ipc_buf, path) };

    let label = procmgr_labels::CREATE_FROM_VFS | ((path.len() as u64) << 16);
    let Ok((reply_label, _)) = syscall::ipc_call(procmgr_ep, label, word_count, &[])
    else
    {
        log("init: phase 3: CREATE_PROCESS_FROM_VFS ipc_call failed");
        return None;
    };
    if reply_label != 0
    {
        log("init: phase 3: CREATE_PROCESS_FROM_VFS failed");
        return None;
    }

    // SAFETY: ipc_buf is the registered IPC buffer, page-aligned.
    #[allow(clippy::cast_ptr_alignment)]
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc_buf) };
    if cap_count < 4
    {
        log("init: phase 3: svcmgr reply missing caps");
        return None;
    }

    Some((reply_caps[0], reply_caps[1], reply_caps[2], reply_caps[3]))
}

/// Inject caps into svcmgr's `CSpace` and patch `ProcessInfo`, then start it.
#[allow(clippy::too_many_arguments)]
pub fn setup_and_start_svcmgr(
    info: &InitInfo,
    procmgr_ep: u32,
    process_handle: u32,
    log_ep: u32,
    svcmgr_service_ep: u32,
    child_cspace: u32,
    pi_frame: u32,
)
{
    let mut desc_buf = [CapDescriptor {
        slot: 0,
        cap_type: CapType::Frame,
        pad: [0; 3],
        aux0: 0,
        aux1: 0,
    }; 8];
    let mut inj = CapInjector::new(&mut desc_buf);

    // Log endpoint.
    inject_cap(
        log_ep,
        syscall::RIGHTS_SEND,
        CapType::Frame,
        LOG_ENDPOINT_SENTINEL,
        0,
        child_cspace,
        &mut inj,
    );

    // Service endpoint (svcmgr receives registrations on this).
    inject_cap(
        svcmgr_service_ep,
        syscall::RIGHTS_ALL,
        CapType::Frame,
        SERVICE_ENDPOINT_SENTINEL,
        0,
        child_cspace,
        &mut inj,
    );

    // procmgr endpoint (svcmgr uses this for restarting services).
    inject_cap(
        procmgr_ep,
        syscall::RIGHTS_SEND_GRANT,
        CapType::Frame,
        PROCMGR_ENDPOINT_SENTINEL,
        0,
        child_cspace,
        &mut inj,
    );

    let count = inj.count;
    let first_slot = inj.first_slot;

    // Patch ProcessInfo.
    if !patch_process_info(
        info.aspace_cap,
        pi_frame,
        &desc_buf,
        count,
        first_slot,
        "init: phase 3: cannot map svcmgr ProcessInfo",
    )
    {
        return;
    }

    // START_PROCESS.
    start_process(
        process_handle,
        "init: phase 3: svcmgr started",
        "init: phase 3: svcmgr START_PROCESS failed",
    );
}

/// Create crasher from its boot module (suspended). Returns `(process_handle, thread_cap, module_cap)`.
pub fn create_crasher_suspended(
    info: &InitInfo,
    procmgr_ep: u32,
    log_ep: u32,
    ipc_buf: *mut u64,
) -> Option<(u32, u32, u32)>
{
    // Crasher is module index 5 (procmgr=0, devmgr=1, vfsd=2, virtio-blk=3, fatfs=4, crasher=5).
    if info.module_frame_count < 6
    {
        log("init: phase 3: no crasher module available");
        return None;
    }

    let crasher_frame_cap = info.module_frame_base + 5;

    // Derive a copy for procmgr's CREATE_PROCESS (IPC moves caps).
    // Keep the original for svcmgr's restart recipe.
    let Ok(frame_for_procmgr) = syscall::cap_derive(crasher_frame_cap, syscall::RIGHTS_ALL)
    else
    {
        log("init: phase 3: cannot derive crasher module cap");
        return None;
    };

    let Ok((reply_label, _)) = syscall::ipc_call(
        procmgr_ep,
        procmgr_labels::CREATE_PROCESS,
        0,
        &[frame_for_procmgr],
    )
    else
    {
        log("init: phase 3: crasher CREATE_PROCESS failed");
        return None;
    };
    if reply_label != 0
    {
        log("init: phase 3: crasher CREATE_PROCESS error");
        return None;
    }

    // SAFETY: ipc_buf is the registered IPC buffer, page-aligned.
    #[allow(clippy::cast_ptr_alignment)]
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc_buf) };
    if cap_count < 4
    {
        log("init: phase 3: crasher reply missing caps");
        return None;
    }
    let process_handle = reply_caps[0];
    let child_cspace = reply_caps[1];
    let pi_frame = reply_caps[2];
    let thread_cap = reply_caps[3];

    // Inject log endpoint into crasher.
    let mut desc_buf = [CapDescriptor {
        slot: 0,
        cap_type: CapType::Frame,
        pad: [0; 3],
        aux0: 0,
        aux1: 0,
    }; 4];
    let mut inj = CapInjector::new(&mut desc_buf);

    inject_cap(
        log_ep,
        syscall::RIGHTS_SEND,
        CapType::Frame,
        LOG_ENDPOINT_SENTINEL,
        0,
        child_cspace,
        &mut inj,
    );

    let count = inj.count;
    let first_slot = inj.first_slot;

    // Patch ProcessInfo.
    if !patch_process_info(
        info.aspace_cap,
        pi_frame,
        &desc_buf,
        count,
        first_slot,
        "init: phase 3: cannot map crasher ProcessInfo",
    )
    {
        return None;
    }

    // Do NOT start — svcmgr must bind death notification before crasher runs.
    log("init: phase 3: crasher created (suspended)");
    Some((process_handle, thread_cap, crasher_frame_cap))
}

/// Register a service with svcmgr via `REGISTER_SERVICE`.
///
/// Sends name, policy, criticality in data words and up to 3 caps
/// (thread, module, `log_ep`) in cap slots.
#[allow(clippy::too_many_arguments)]
pub fn register_service(
    svcmgr_ep: u32,
    ipc_buf: *mut u64,
    name: &[u8],
    restart_policy: u8,
    criticality: u8,
    thread_cap: u32,
    module_cap: u32,
    log_ep: u32,
)
{
    // data[0] = restart_policy, data[1] = criticality, data[2..] = name packed.
    // SAFETY: ipc_buf is the registered IPC buffer.
    unsafe { core::ptr::write_volatile(ipc_buf, u64::from(restart_policy)) };
    // SAFETY: ipc_buf offset 1.
    unsafe { core::ptr::write_volatile(ipc_buf.add(1), u64::from(criticality)) };

    let name_words = name.len().div_ceil(8);
    for w in 0..name_words
    {
        let mut word: u64 = 0;
        for b in 0..8
        {
            let idx = w * 8 + b;
            if idx < name.len()
            {
                word |= u64::from(name[idx]) << (b * 8);
            }
        }
        // SAFETY: ipc_buf is valid; writing name word at offset 2+w.
        unsafe { core::ptr::write_volatile(ipc_buf.add(2 + w), word) };
    }

    let data_count = 2 + name_words;
    let label = svcmgr_labels::REGISTER_SERVICE | ((name.len() as u64) << 16);

    // Build cap list. For Fatal services, module_cap and log_ep may be 0.
    let mut caps = [0u32; 3];
    let mut cap_count = 0;
    if thread_cap != 0
    {
        caps[cap_count] = thread_cap;
        cap_count += 1;
    }
    if module_cap != 0
    {
        caps[cap_count] = module_cap;
        cap_count += 1;
    }
    if log_ep != 0 && module_cap != 0
    {
        // Only send log_ep if service is restartable (has module_cap).
        if let Ok(derived) = syscall::cap_derive(log_ep, syscall::RIGHTS_SEND)
        {
            caps[cap_count] = derived;
            cap_count += 1;
        }
    }

    match syscall::ipc_call(svcmgr_ep, label, data_count, &caps[..cap_count])
    {
        Ok((0, _)) =>
        {}
        _ => log("init: phase 3: REGISTER_SERVICE failed"),
    }
}

/// Phase 3: create svcmgr from VFS, register services, start crasher, handover.
#[allow(clippy::too_many_lines)]
pub fn phase3_svcmgr_handover(
    info: &InitInfo,
    procmgr_ep: u32,
    log_ep: u32,
    vfsd_service_ep: u32,
    ipc_buf: *mut u64,
) -> !
{
    // 1. Send vfsd endpoint to procmgr for VFS-based ELF loading.
    send_vfsd_endpoint_to_procmgr(procmgr_ep, vfsd_service_ep);

    // 2. Create svcmgr service endpoint.
    let Ok(svcmgr_service_ep) = syscall::cap_create_endpoint()
    else
    {
        log("init: phase 3: cannot create svcmgr endpoint");
        idle_loop();
    };

    // 3. Create svcmgr from VFS.
    log("init: phase 3: loading svcmgr from /bin/svcmgr");
    let Some((svcmgr_handle, svcmgr_cspace, svcmgr_pi, _svcmgr_thread)) =
        create_svcmgr_from_vfs(procmgr_ep, ipc_buf)
    else
    {
        log("init: phase 3: failed to create svcmgr, idling");
        idle_loop();
    };

    // 4. Inject caps and start svcmgr.
    setup_and_start_svcmgr(
        info,
        procmgr_ep,
        svcmgr_handle,
        log_ep,
        svcmgr_service_ep,
        svcmgr_cspace,
        svcmgr_pi,
    );

    // 5. Create crasher (suspended — don't start until svcmgr is monitoring).
    let crasher = create_crasher_suspended(info, procmgr_ep, log_ep, ipc_buf);

    // 6. Register services with svcmgr.
    log("init: phase 3: registering services with svcmgr");

    // procmgr: Fatal, Never restart. Thread cap not available from bootstrap_procmgr
    // (it was created via raw syscalls, not procmgr IPC). Skip for now — procmgr
    // crash is unrecoverable regardless.

    // crasher: Normal, Always restart.
    if let Some((crasher_handle, crasher_thread, crasher_module)) = crasher
    {
        register_service(
            svcmgr_service_ep,
            ipc_buf,
            b"crasher",
            0, // POLICY_ALWAYS
            1, // CRITICALITY_NORMAL
            crasher_thread,
            crasher_module,
            log_ep,
        );

        // 7. Start crasher now that svcmgr is monitoring.
        start_process(
            crasher_handle,
            "init: phase 3: crasher started",
            "init: phase 3: crasher START_PROCESS failed",
        );
    }

    // 8. HANDOVER_COMPLETE.
    match syscall::ipc_call(svcmgr_service_ep, svcmgr_labels::HANDOVER_COMPLETE, 0, &[])
    {
        Ok((0, _)) => log("init: phase 3: handover complete"),
        _ => log("init: phase 3: handover failed"),
    }

    log("init: main thread exiting, log thread continues");
    syscall::thread_exit();
}
