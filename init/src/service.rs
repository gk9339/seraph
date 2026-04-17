// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// init/src/service.rs

//! Service creation helpers for init.
//!
//! Creates suspended child processes via procmgr IPC (`CREATE_PROCESS` /
//! `CREATE_FROM_VFS`), starts them, then serves their bootstrap requests on
//! init's bootstrap endpoint to deliver their per-service capability set.

use crate::bootstrap::NEXT_BOOTSTRAP_TOKEN;
use crate::idle_loop;
use crate::logging::log;
use init_protocol::{CapDescriptor, CapType, InitInfo};
use ipc::{procmgr_labels, svcmgr_labels, write_path_to_ipc, IpcBuf};

// ── Helpers ─────────────────────────────────────────────────────────────────

fn derive_tokened_creator(bootstrap_ep: u32) -> Option<(u32, u64)>
{
    let token = NEXT_BOOTSTRAP_TOKEN.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    let tokened = syscall::cap_derive_token(bootstrap_ep, syscall::RIGHTS_SEND, token).ok()?;
    Some((tokened, token))
}

/// Start a process by calling `START_PROCESS` on its tokened process handle.
fn start_process(process_handle: u32, ok_msg: &str, fail_msg: &str) -> bool
{
    if let Ok((0, _)) = syscall::ipc_call(process_handle, procmgr_labels::START_PROCESS, 0, &[])
    {
        log(ok_msg);
        true
    }
    else
    {
        log(fail_msg);
        false
    }
}

/// Serve one bootstrap round from init to the named child.
fn serve(
    bootstrap_ep: u32,
    token: u64,
    ipc: IpcBuf,
    done: bool,
    caps: &[u32],
    data: &[u64],
    context: &str,
) -> bool
{
    if ipc::bootstrap::serve_round(bootstrap_ep, token, ipc, done, caps, data).is_err()
    {
        log(context);
        return false;
    }
    true
}

// ── Hardware cap partitioning for devmgr ────────────────────────────────────

/// Collected hardware caps from init's kernel-delivered `CapDescriptor` table.
struct HwCaps
{
    ecam_slot: u32,
    ecam_base: u64,
    ecam_size: u64,
    mmio_windows: [(u32, u64, u64); 2], // (slot, base, size)
    mmio_count: usize,
    irqs: [(u32, u32); 64], // (slot, irq_id)
    irq_count: usize,
}

impl HwCaps
{
    const fn new() -> Self
    {
        Self {
            ecam_slot: 0,
            ecam_base: 0,
            ecam_size: 0,
            mmio_windows: [(0, 0, 0); 2],
            mmio_count: 0,
            irqs: [(0, 0); 64],
            irq_count: 0,
        }
    }
}

fn collect_hw_caps(init_descs: &[CapDescriptor]) -> HwCaps
{
    let mut hw = HwCaps::new();
    for d in init_descs
    {
        match d.cap_type
        {
            CapType::PciEcam =>
            {
                hw.ecam_slot = d.slot;
                hw.ecam_base = d.aux0;
                hw.ecam_size = d.aux1;
            }
            CapType::MmioRegion if d.aux1 >= 0x1000_0000 && hw.mmio_count < 2 =>
            {
                hw.mmio_windows[hw.mmio_count] = (d.slot, d.aux0, d.aux1);
                hw.mmio_count += 1;
            }
            CapType::Interrupt if hw.irq_count < hw.irqs.len() =>
            {
                hw.irqs[hw.irq_count] = (d.slot, d.aux0 as u32);
                hw.irq_count += 1;
            }
            _ =>
            {}
        }
    }
    hw
}

// ── devmgr creation ──────────────────────────────────────────────────────────

/// Create devmgr via procmgr and serve its bootstrap (hardware caps).
///
/// The bootstrap layout mirrors `devmgr/src/caps.rs::bootstrap_caps`.
// clippy::too_many_lines: delivery of devmgr's initial cap set is a
// transaction — derive each cap, package it with the collected hardware
// descriptors, then serve two bootstrap rounds. All the per-cap derive
// sites must unwind cooperatively on failure; splitting requires threading
// the partial-state rollback through multiple helpers with no gain in
// clarity over the inline, linear presentation.
#[allow(clippy::too_many_lines)]
pub fn create_devmgr_with_caps(
    info: &InitInfo,
    procmgr_ep: u32,
    bootstrap_ep: u32,
    log_ep: u32,
    registry_ep: u32,
    ipc: IpcBuf,
)
{
    let devmgr_frame_cap = info.module_frame_base + 1;

    let Some((tokened_creator, child_token)) = derive_tokened_creator(bootstrap_ep)
    else
    {
        log("init: devmgr: token derivation failed");
        return;
    };

    let Ok((reply_label, _)) = syscall::ipc_call(
        procmgr_ep,
        procmgr_labels::CREATE_PROCESS,
        0,
        &[devmgr_frame_cap, tokened_creator],
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

    // SAFETY: ipc is the registered IPC buffer.
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc.as_ptr()) };
    if cap_count < 1
    {
        log("init: devmgr: CREATE_PROCESS reply missing caps");
        return;
    }
    let process_handle = reply_caps[0];

    let hw = collect_hw_caps(crate::descriptors(info));

    // Derive all caps for delivery.
    let Ok(log_copy) = syscall::cap_derive(log_ep, syscall::RIGHTS_SEND)
    else
    {
        log("init: devmgr: log cap derive failed");
        return;
    };
    let Ok(registry_copy) = syscall::cap_derive(registry_ep, syscall::RIGHTS_ALL)
    else
    {
        log("init: devmgr: registry cap derive failed");
        return;
    };
    let Ok(procmgr_copy) = syscall::cap_derive(procmgr_ep, syscall::RIGHTS_SEND_GRANT)
    else
    {
        log("init: devmgr: procmgr cap derive failed");
        return;
    };
    let Ok(ecam_copy) = syscall::cap_derive(hw.ecam_slot, syscall::RIGHTS_ALL)
    else
    {
        log("init: devmgr: ecam cap derive failed");
        return;
    };

    // START_PROCESS.
    if !start_process(
        process_handle,
        "init: devmgr started; serving bootstrap",
        "init: devmgr: START_PROCESS failed",
    )
    {
        return;
    }

    // Round 1: [log, registry, procmgr, ecam]; data [ecam_base, ecam_size].
    if !serve(
        bootstrap_ep,
        child_token,
        ipc,
        false,
        &[log_copy, registry_copy, procmgr_copy, ecam_copy],
        &[hw.ecam_base, hw.ecam_size],
        "init: devmgr: bootstrap round 1 failed",
    )
    {
        return;
    }

    // Round 2: MMIO windows. Data: [count, base0, size0, base1, size1].
    let mut mmio_caps = [0u32; 2];
    let mut mmio_data = [0u64; 5];
    mmio_data[0] = hw.mmio_count as u64;
    for i in 0..hw.mmio_count
    {
        let (slot, base, size) = hw.mmio_windows[i];
        if let Ok(c) = syscall::cap_derive(slot, syscall::RIGHTS_ALL)
        {
            mmio_caps[i] = c;
        }
        mmio_data[1 + i * 2] = base;
        mmio_data[2 + i * 2] = size;
    }

    let done_after_mmio = hw.irq_count == 0 && info.module_frame_count <= 3;

    if !serve(
        bootstrap_ep,
        child_token,
        ipc,
        done_after_mmio,
        &mmio_caps[..hw.mmio_count],
        &mmio_data[..=hw.mmio_count * 2],
        "init: devmgr: bootstrap round 2 (MMIO) failed",
    )
    {
        return;
    }
    if done_after_mmio
    {
        return;
    }

    // IRQ rounds: 4 caps per round with kind=0 tag.
    let mut irq_idx = 0;
    while irq_idx < hw.irq_count
    {
        let batch_end = (irq_idx + 4).min(hw.irq_count);
        let batch_count = batch_end - irq_idx;
        let mut irq_caps = [0u32; 4];
        let mut irq_data = [0u64; 5];
        irq_data[0] = 0; // kind=0 (IRQ round)
        for j in 0..batch_count
        {
            let (slot, id) = hw.irqs[irq_idx + j];
            if let Ok(c) = syscall::cap_derive(slot, syscall::RIGHTS_ALL)
            {
                irq_caps[j] = c;
            }
            irq_data[1 + j] = u64::from(id);
        }

        let is_last_irq = batch_end == hw.irq_count;
        let done_here = is_last_irq && info.module_frame_count <= 3;

        if !serve(
            bootstrap_ep,
            child_token,
            ipc,
            done_here,
            &irq_caps[..batch_count],
            &irq_data[..=batch_count],
            "init: devmgr: bootstrap IRQ round failed",
        )
        {
            return;
        }
        if done_here
        {
            return;
        }
        irq_idx = batch_end;
    }

    // Module rounds: driver module frames (virtio-blk = module 3).
    if info.module_frame_count > 3
    {
        let module_cap = info.module_frame_base + 3;
        let Ok(module_copy) = syscall::cap_derive(module_cap, syscall::RIGHTS_ALL)
        else
        {
            log("init: devmgr: module cap derive failed");
            return;
        };

        // kind=1 (module round), one cap.
        let _ = serve(
            bootstrap_ep,
            child_token,
            ipc,
            true,
            &[module_copy],
            &[1u64],
            "init: devmgr: bootstrap module round failed",
        );
    }
}

// ── vfsd creation ────────────────────────────────────────────────────────────

/// Endpoint set passed to vfsd via its bootstrap round.
#[allow(clippy::struct_field_names)]
pub struct VfsdSpawnCaps
{
    pub log_ep: u32,
    pub registry_ep: u32,
    pub vfsd_service_ep: u32,
}

/// Create vfsd via procmgr and serve its bootstrap.
pub fn create_vfsd_with_caps(
    info: &InitInfo,
    procmgr_ep: u32,
    bootstrap_ep: u32,
    spawn: &VfsdSpawnCaps,
    ipc: IpcBuf,
)
{
    let vfsd_frame_cap = info.module_frame_base + 2;

    let Some((tokened_creator, child_token)) = derive_tokened_creator(bootstrap_ep)
    else
    {
        log("init: vfsd: token derivation failed");
        return;
    };

    let Ok((reply_label, _)) = syscall::ipc_call(
        procmgr_ep,
        procmgr_labels::CREATE_PROCESS,
        0,
        &[vfsd_frame_cap, tokened_creator],
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

    // SAFETY: ipc is the registered IPC buffer.
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc.as_ptr()) };
    if cap_count < 1
    {
        log("init: vfsd: CREATE_PROCESS reply missing caps");
        return;
    }
    let process_handle = reply_caps[0];

    let Ok(log_copy) = syscall::cap_derive(spawn.log_ep, syscall::RIGHTS_SEND)
    else
    {
        return;
    };
    let Ok(service_copy) = syscall::cap_derive(spawn.vfsd_service_ep, syscall::RIGHTS_ALL)
    else
    {
        return;
    };
    let Ok(registry_copy) = syscall::cap_derive(spawn.registry_ep, syscall::RIGHTS_SEND)
    else
    {
        return;
    };
    let Ok(procmgr_copy) = syscall::cap_derive(procmgr_ep, syscall::RIGHTS_SEND_GRANT)
    else
    {
        return;
    };

    if !start_process(
        process_handle,
        "init: vfsd started; serving bootstrap",
        "init: vfsd: START_PROCESS failed",
    )
    {
        return;
    }

    // Round 1: [log, service, registry, procmgr]
    if !serve(
        bootstrap_ep,
        child_token,
        ipc,
        false,
        &[log_copy, service_copy, registry_copy, procmgr_copy],
        &[],
        "init: vfsd: bootstrap round 1 failed",
    )
    {
        return;
    }

    // Round 2: fatfs module.
    let fatfs_cap = if info.module_frame_count > 4
    {
        syscall::cap_derive(info.module_frame_base + 4, syscall::RIGHTS_ALL).unwrap_or(0)
    }
    else
    {
        0
    };

    let _ = serve(
        bootstrap_ep,
        child_token,
        ipc,
        true,
        &[fatfs_cap],
        &[],
        "init: vfsd: bootstrap round 2 failed",
    );
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

/// Create svcmgr from VFS (`/bin/svcmgr`) via `CREATE_FROM_VFS`.
///
/// Returns `(process_handle, child_token)` on success.
pub fn create_svcmgr_from_vfs(procmgr_ep: u32, bootstrap_ep: u32, ipc: IpcBuf)
    -> Option<(u32, u64)>
{
    let path = b"/bin/svcmgr";

    let word_count = write_path_to_ipc(ipc, path);

    let (tokened_creator, child_token) = derive_tokened_creator(bootstrap_ep)?;

    let label = procmgr_labels::CREATE_FROM_VFS | ((path.len() as u64) << 16);
    let Ok((reply_label, _)) = syscall::ipc_call(procmgr_ep, label, word_count, &[tokened_creator])
    else
    {
        log("init: phase 3: CREATE_FROM_VFS ipc_call failed");
        return None;
    };
    if reply_label != 0
    {
        log("init: phase 3: CREATE_FROM_VFS failed");
        return None;
    }

    // SAFETY: ipc is the registered IPC buffer.
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc.as_ptr()) };
    if cap_count < 1
    {
        log("init: phase 3: svcmgr reply missing caps");
        return None;
    }

    Some((reply_caps[0], child_token))
}

/// Endpoint set handed to svcmgr in its bootstrap round.
#[allow(clippy::struct_field_names)]
pub struct SvcmgrHandoverCaps
{
    pub log_ep: u32,
    pub svcmgr_service_ep: u32,
    pub svcmgr_bootstrap_ep: u32,
}

/// Start svcmgr, then serve its bootstrap.
pub fn setup_and_start_svcmgr(
    procmgr_ep: u32,
    bootstrap_ep: u32,
    process_handle: u32,
    child_token: u64,
    handover: &SvcmgrHandoverCaps,
    ipc: IpcBuf,
)
{
    if !start_process(
        process_handle,
        "init: phase 3: svcmgr started; serving bootstrap",
        "init: phase 3: svcmgr START_PROCESS failed",
    )
    {
        return;
    }

    let Ok(log_copy) = syscall::cap_derive(handover.log_ep, syscall::RIGHTS_SEND)
    else
    {
        return;
    };
    let Ok(service_copy) = syscall::cap_derive(handover.svcmgr_service_ep, syscall::RIGHTS_ALL)
    else
    {
        return;
    };
    let Ok(procmgr_copy) = syscall::cap_derive(procmgr_ep, syscall::RIGHTS_SEND_GRANT)
    else
    {
        return;
    };
    let Ok(boot_copy) = syscall::cap_derive(handover.svcmgr_bootstrap_ep, syscall::RIGHTS_ALL)
    else
    {
        return;
    };

    // One round: [log, service, procmgr, bootstrap_ep].
    let _ = serve(
        bootstrap_ep,
        child_token,
        ipc,
        true,
        &[log_copy, service_copy, procmgr_copy, boot_copy],
        &[],
        "init: phase 3: svcmgr bootstrap failed",
    );
}

/// Create crasher from its boot module (suspended).
/// Returns `(process_handle, thread_cap, module_cap, child_token)`.
pub fn create_crasher_suspended(
    info: &InitInfo,
    procmgr_ep: u32,
    bootstrap_ep: u32,
    ipc: IpcBuf,
) -> Option<(u32, u32, u32, u64)>
{
    if info.module_frame_count < 6
    {
        log("init: phase 3: no crasher module available");
        return None;
    }

    let crasher_frame_cap = info.module_frame_base + 5;
    let frame_for_procmgr = syscall::cap_derive(crasher_frame_cap, syscall::RIGHTS_ALL).ok()?;

    let (tokened_creator, child_token) = derive_tokened_creator(bootstrap_ep)?;

    let Ok((reply_label, _)) = syscall::ipc_call(
        procmgr_ep,
        procmgr_labels::CREATE_PROCESS,
        0,
        &[frame_for_procmgr, tokened_creator],
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

    // SAFETY: ipc is the registered IPC buffer.
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc.as_ptr()) };
    if cap_count < 2
    {
        log("init: phase 3: crasher reply missing caps");
        return None;
    }

    let process_handle = reply_caps[0];
    let thread_cap = reply_caps[1];

    log("init: phase 3: crasher created (suspended)");
    Some((process_handle, thread_cap, crasher_frame_cap, child_token))
}

/// Create allocsmoke from its boot module (suspended), start it, and serve
/// its bootstrap with `[log_ep, procmgr_ep]`. allocsmoke exits cleanly on
/// completion and is not registered with svcmgr.
pub fn create_and_run_allocsmoke(
    info: &InitInfo,
    procmgr_ep: u32,
    bootstrap_ep: u32,
    log_ep: u32,
    ipc: IpcBuf,
)
{
    if info.module_frame_count < 7
    {
        return;
    }
    let module_frame = info.module_frame_base + 6;
    let Ok(frame_for_procmgr) = syscall::cap_derive(module_frame, syscall::RIGHTS_ALL)
    else
    {
        return;
    };

    let Some((tokened_creator, child_token)) = derive_tokened_creator(bootstrap_ep)
    else
    {
        return;
    };

    let Ok((reply_label, _)) = syscall::ipc_call(
        procmgr_ep,
        procmgr_labels::CREATE_PROCESS,
        0,
        &[frame_for_procmgr, tokened_creator],
    )
    else
    {
        return;
    };
    if reply_label != 0
    {
        return;
    }

    // SAFETY: ipc wraps the registered IPC buffer.
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc.as_ptr()) };
    if cap_count < 1
    {
        return;
    }
    let process_handle = reply_caps[0];

    let log_copy = syscall::cap_derive(log_ep, syscall::RIGHTS_SEND).unwrap_or(0);
    let procmgr_copy = syscall::cap_derive(procmgr_ep, syscall::RIGHTS_SEND_GRANT).unwrap_or(0);

    if !start_process(
        process_handle,
        "init: phase 3: allocsmoke started",
        "init: phase 3: allocsmoke START_PROCESS failed",
    )
    {
        return;
    }

    let _ = serve(
        bootstrap_ep,
        child_token,
        ipc,
        true,
        &[log_copy, procmgr_copy],
        &[],
        "init: phase 3: allocsmoke bootstrap failed",
    );
}

/// Start crasher and serve its bootstrap with `[log_ep, svcmgr_ep]`.
///
/// `svcmgr_service_ep` is the same cap that svcmgr will re-inject from the
/// restart bundle under the name `"svcmgr"`. Providing it on first boot as
/// well keeps the cap layout identical across first-boot and restart paths,
/// so crasher sees the same `cap_count` and entry in both.
pub fn start_and_bootstrap_crasher(
    process_handle: u32,
    child_token: u64,
    bootstrap_ep: u32,
    log_ep: u32,
    svcmgr_service_ep: u32,
    ipc: IpcBuf,
) -> bool
{
    if !start_process(
        process_handle,
        "init: phase 3: crasher started",
        "init: phase 3: crasher START_PROCESS failed",
    )
    {
        return false;
    }

    let log_copy = if log_ep != 0
    {
        syscall::cap_derive(log_ep, syscall::RIGHTS_SEND).unwrap_or(0)
    }
    else
    {
        0
    };
    let svcmgr_copy = if svcmgr_service_ep != 0
    {
        syscall::cap_derive(svcmgr_service_ep, syscall::RIGHTS_SEND).unwrap_or(0)
    }
    else
    {
        0
    };

    serve(
        bootstrap_ep,
        child_token,
        ipc,
        true,
        &[log_copy, svcmgr_copy],
        &[],
        "init: phase 3: crasher bootstrap failed",
    )
}

/// One service's registration data, passed to `register_service`.
pub struct ServiceRegistration<'a>
{
    pub name: &'a [u8],
    pub restart_policy: u8,
    pub criticality: u8,
    pub thread_cap: u32,
    pub module_cap: u32,
    pub log_ep: u32,
    /// Optional extra named cap for svcmgr's restart bundle. If both
    /// `bundle_name` is non-empty and `bundle_cap != 0`, the cap will be
    /// re-injected into every restart of this service under the given name.
    pub bundle_name: &'a [u8],
    pub bundle_cap: u32,
}

/// Register a service with svcmgr via `REGISTER_SERVICE`.
pub fn register_service(svcmgr_ep: u32, ipc: IpcBuf, reg: &ServiceRegistration)
{
    ipc.write_word(0, u64::from(reg.restart_policy));
    ipc.write_word(1, u64::from(reg.criticality));

    let name_words = reg.name.len().div_ceil(8);
    for w in 0..name_words
    {
        let mut word: u64 = 0;
        for b in 0..8
        {
            let idx = w * 8 + b;
            if idx < reg.name.len()
            {
                word |= u64::from(reg.name[idx]) << (b * 8);
            }
        }
        ipc.write_word(2 + w, word);
    }

    // Bundle-name tail: [bundle_name_len, bundle_name_words...] packed after
    // the service name. Zero if no bundle cap is being sent.
    let bundle_name_len_word = 2 + name_words;
    let include_bundle = reg.module_cap != 0
        && reg.log_ep != 0
        && reg.bundle_cap != 0
        && !reg.bundle_name.is_empty()
        && reg.bundle_name.len() <= 16;
    let bundle_name_len = if include_bundle
    {
        reg.bundle_name.len()
    }
    else
    {
        0
    };
    ipc.write_word(bundle_name_len_word, bundle_name_len as u64);
    let bundle_name_words = bundle_name_len.div_ceil(8);
    for w in 0..bundle_name_words
    {
        let mut word: u64 = 0;
        for b in 0..8
        {
            let idx = w * 8 + b;
            if idx < bundle_name_len
            {
                word |= u64::from(reg.bundle_name[idx]) << (b * 8);
            }
        }
        ipc.write_word(bundle_name_len_word + 1 + w, word);
    }

    let data_count = bundle_name_len_word + 1 + bundle_name_words;
    let label = svcmgr_labels::REGISTER_SERVICE | ((reg.name.len() as u64) << 16);

    let mut caps = [0u32; 4];
    let mut cap_count = 0;
    if reg.thread_cap != 0
    {
        caps[cap_count] = reg.thread_cap;
        cap_count += 1;
    }
    if reg.module_cap != 0
    {
        caps[cap_count] = reg.module_cap;
        cap_count += 1;
    }
    if reg.log_ep != 0 && reg.module_cap != 0
    {
        if let Ok(derived) = syscall::cap_derive(reg.log_ep, syscall::RIGHTS_SEND)
        {
            caps[cap_count] = derived;
            cap_count += 1;
        }
    }
    if include_bundle
    {
        if let Ok(derived) = syscall::cap_derive(reg.bundle_cap, syscall::RIGHTS_SEND)
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

// ── Phase 3 orchestration ───────────────────────────────────────────────────

/// Phase 3: create svcmgr from VFS, register services, start crasher, handover.
// clippy::too_many_lines: svcmgr handover is a single transaction that owns
// the in-flight tokens for svcmgr and crasher processes; the partial-state
// unwind on any failure (svcmgr creation fails, crasher creation fails,
// registration fails, HANDOVER_COMPLETE fails) must see every token in
// scope. Factoring into helpers requires threading every token through each,
// which regresses readability.
#[allow(clippy::too_many_lines)]
pub fn phase3_svcmgr_handover(
    info: &InitInfo,
    procmgr_ep: u32,
    bootstrap_ep: u32,
    log_ep: u32,
    vfsd_service_ep: u32,
    ipc: IpcBuf,
) -> !
{
    let _ = info;

    send_vfsd_endpoint_to_procmgr(procmgr_ep, vfsd_service_ep);

    let Ok(svcmgr_service_ep) = syscall::cap_create_endpoint()
    else
    {
        log("init: phase 3: cannot create svcmgr endpoint");
        idle_loop();
    };
    let Ok(svcmgr_bootstrap_ep) = syscall::cap_create_endpoint()
    else
    {
        log("init: phase 3: cannot create svcmgr bootstrap endpoint");
        idle_loop();
    };

    log("init: phase 3: loading svcmgr from /bin/svcmgr");
    let Some((svcmgr_handle, svcmgr_token)) = create_svcmgr_from_vfs(procmgr_ep, bootstrap_ep, ipc)
    else
    {
        log("init: phase 3: failed to create svcmgr, idling");
        idle_loop();
    };

    let handover = SvcmgrHandoverCaps {
        log_ep,
        svcmgr_service_ep,
        svcmgr_bootstrap_ep,
    };
    setup_and_start_svcmgr(
        procmgr_ep,
        bootstrap_ep,
        svcmgr_handle,
        svcmgr_token,
        &handover,
        ipc,
    );

    let crasher = create_crasher_suspended(info, procmgr_ep, bootstrap_ep, ipc);

    log("init: phase 3: registering services with svcmgr");

    if let Some((crasher_handle, crasher_thread, crasher_module, crasher_token)) = crasher
    {
        register_service(
            svcmgr_service_ep,
            ipc,
            &ServiceRegistration {
                name: b"crasher",
                restart_policy: 0, // POLICY_ALWAYS
                criticality: 1,    // CRITICALITY_NORMAL
                thread_cap: crasher_thread,
                module_cap: crasher_module,
                log_ep,
                bundle_name: b"svcmgr",
                bundle_cap: svcmgr_service_ep,
            },
        );

        start_and_bootstrap_crasher(
            crasher_handle,
            crasher_token,
            bootstrap_ep,
            log_ep,
            svcmgr_service_ep,
            ipc,
        );
    }

    // Spawn allocsmoke (run-once test; no svcmgr registration).
    create_and_run_allocsmoke(info, procmgr_ep, bootstrap_ep, log_ep, ipc);

    match syscall::ipc_call(svcmgr_service_ep, svcmgr_labels::HANDOVER_COMPLETE, 0, &[])
    {
        Ok((0, _)) => log("init: phase 3: handover complete"),
        _ => log("init: phase 3: handover failed"),
    }

    log("init: main thread exiting, log thread continues");
    syscall::thread_exit();
}
