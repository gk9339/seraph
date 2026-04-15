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
#![allow(clippy::cast_possible_truncation)]

extern crate runtime;

use process_abi::{CapDescriptor, CapType, ProcessInfo, StartupInfo};

// ── Constants ────────────────────────────────────────────────────────────────

const PAGE_SIZE: u64 = 0x1000;

/// Maximum number of monitored services.
const MAX_SERVICES: usize = 16;

/// Maximum restart attempts before marking degraded.
const MAX_RESTARTS: u32 = 5;

/// IPC label for `REGISTER_SERVICE`.
const LABEL_REGISTER: u64 = 1;

/// IPC label for `HANDOVER_COMPLETE`.
const LABEL_HANDOVER: u64 = 2;

/// IPC label for `CREATE_PROCESS` (procmgr).
const LABEL_CREATE_PROCESS: u64 = 1;

/// IPC label for `START_PROCESS` (procmgr).
const LABEL_START_PROCESS: u64 = 2;

/// Sentinel value in `CapDescriptor.aux0` indicating a log endpoint.
const LOG_ENDPOINT_SENTINEL: u64 = 0xFFFF_FFFF_FFFF_FFFF;

/// Sentinel value in `CapDescriptor.aux0` indicating a service endpoint.
const SERVICE_ENDPOINT_SENTINEL: u64 = 0xFFFF_FFFF_FFFF_FFFE;

/// Sentinel value in `CapDescriptor.aux0` indicating a procmgr endpoint.
const PROCMGR_ENDPOINT_SENTINEL: u64 = 0xFFFF_FFFF_FFFF_FFFB;

/// VA for mapping child `ProcessInfo` frames during cap injection on restart.
const CHILD_PI_VA: u64 = 0x0000_0002_0000_0000; // 8 GiB

/// Restart policy: restart unconditionally on any exit.
const POLICY_ALWAYS: u8 = 0;

/// Restart policy: restart only on fault (nonzero exit reason).
const POLICY_ON_FAILURE: u8 = 1;

// Restart policy: never restart.
// const POLICY_NEVER: u8 = 2;

/// Criticality: crash of this service is fatal — halt the system.
const CRITICALITY_FATAL: u8 = 0;

/// Criticality: crash can be handled by restart policy.
const CRITICALITY_NORMAL: u8 = 1;

// ── Service table ────────────────────────────────────────────────────────────

struct ServiceEntry
{
    name: [u8; 32],
    name_len: u8,
    thread_cap: u32,
    module_cap: u32,
    log_ep_cap: u32,
    restart_policy: u8,
    criticality: u8,
    event_queue_cap: u32,
    restart_count: u32,
    active: bool,
}

impl ServiceEntry
{
    const fn empty() -> Self
    {
        Self {
            name: [0; 32],
            name_len: 0,
            thread_cap: 0,
            module_cap: 0,
            log_ep_cap: 0,
            restart_policy: 0,
            criticality: 0,
            event_queue_cap: 0,
            restart_count: 0,
            active: false,
        }
    }

    fn name_str(&self) -> &str
    {
        core::str::from_utf8(&self.name[..self.name_len as usize]).unwrap_or("???")
    }
}

// ── Cap classification ──────────────────────────────────────────────────────

struct SvcmgrCaps
{
    log_ep: u32,
    service_ep: u32,
    procmgr_ep: u32,
    self_aspace: u32,
}

fn classify_caps(startup: &StartupInfo) -> SvcmgrCaps
{
    let mut caps = SvcmgrCaps {
        log_ep: 0,
        service_ep: 0,
        procmgr_ep: 0,
        self_aspace: startup.self_aspace,
    };

    for d in startup.initial_caps
    {
        if d.cap_type == CapType::Frame
        {
            if d.aux0 == LOG_ENDPOINT_SENTINEL
            {
                caps.log_ep = d.slot;
            }
            else if d.aux0 == SERVICE_ENDPOINT_SENTINEL
            {
                caps.service_ep = d.slot;
            }
            else if d.aux0 == PROCMGR_ENDPOINT_SENTINEL
            {
                caps.procmgr_ep = d.slot;
            }
        }
    }

    caps
}

// ── Registration handling ───────────────────────────────────────────────────

/// Handle a `REGISTER_SERVICE` IPC message.
///
/// Reads name, policy, criticality from data words. Reads `thread_cap`,
/// `module_cap`, `log_ep` from transferred caps. Creates an event queue, binds
/// it to the thread, adds to the wait set.
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
        return 2; // InvalidName
    }
    if *service_count >= MAX_SERVICES
    {
        return 1; // TableFull
    }

    // SAFETY: ipc_buf is the registered IPC buffer; data words at offsets 0..N.
    let restart_policy = unsafe { core::ptr::read_volatile(ipc_buf) } as u8;
    // SAFETY: ipc_buf is the registered IPC buffer; offset 1 is within bounds.
    let criticality = unsafe { core::ptr::read_volatile(ipc_buf.add(1)) } as u8;

    // Read name bytes packed into u64 words starting at data[2].
    let mut name = [0u8; 32];
    let name_words = name_len.div_ceil(8);
    for w in 0..name_words
    {
        // SAFETY: ipc_buf is valid; reading data word at offset 2+w.
        let word = unsafe { core::ptr::read_volatile(ipc_buf.add(2 + w)) };
        for b in 0..8
        {
            let idx = w * 8 + b;
            if idx < name_len
            {
                name[idx] = (word >> (b * 8)) as u8;
            }
        }
    }

    // Read transferred caps: cap[0]=thread, cap[1]=module (optional), cap[2]=log_ep (optional).
    // SAFETY: ipc_buf is the registered IPC buffer, page-aligned.
    let (cap_count, recv_caps) = unsafe { syscall::read_recv_caps(ipc_buf.cast::<u64>()) };

    let thread_cap = if cap_count >= 1 { recv_caps[0] } else { 0 };
    let module_cap = if cap_count >= 2 { recv_caps[1] } else { 0 };
    let log_ep_cap = if cap_count >= 3 { recv_caps[2] } else { 0 };

    if thread_cap == 0
    {
        return 3;
    }

    let Ok(eq_cap) = syscall::event_queue_create(4)
    else
    {
        runtime::log!("svcmgr: failed to create event queue for service");
        return 4;
    };

    if syscall::thread_bind_notification(thread_cap, eq_cap).is_err()
    {
        runtime::log!("svcmgr: failed to bind death notification");
        return 5;
    }

    // Token = service_index + 1 (token 0 = service endpoint).
    let token = (*service_count as u64) + 1;
    if syscall::wait_set_add(ws_cap, eq_cap, token).is_err()
    {
        runtime::log!("svcmgr: failed to add event queue to wait set");
        return 6;
    }

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
    };
    *service_count += 1;

    runtime::log!("svcmgr: registered service: {}", services[idx].name_str());

    0 // success
}

// ── Death handling and restart ──────────────────────────────────────────────

/// Handle a service death detected via event queue notification.
#[allow(clippy::too_many_lines)]
fn handle_death(
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

    let should_restart = match svc.restart_policy
    {
        POLICY_ALWAYS => true,
        POLICY_ON_FAILURE => exit_reason != 0,
        _ => false, // POLICY_NEVER or unknown
    };

    if !should_restart
    {
        runtime::log!("svcmgr: restart policy says no restart");
        svc.active = false;
        return;
    }

    if svc.restart_count >= MAX_RESTARTS
    {
        runtime::log!("svcmgr: max restarts reached, marking degraded");
        svc.active = false;
        return;
    }

    if svc.module_cap == 0
    {
        runtime::log!("svcmgr: no module cap, cannot restart");
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

/// Create a new process instance, inject caps, start it, and rebind death
/// notification. Returns `true` on success.
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
    // 1. CREATE_PROCESS via procmgr. Derive a copy of the module cap so we
    //    retain the original for future restarts (IPC cap transfer consumes it).
    let Ok(module_copy) = syscall::cap_derive(svc.module_cap, !0u64)
    else
    {
        runtime::log!("svcmgr: restart cap_derive failed");
        return false;
    };
    let Ok((reply_label, _)) =
        syscall::ipc_call(procmgr_ep, LABEL_CREATE_PROCESS, 0, &[module_copy])
    else
    {
        runtime::log!("svcmgr: restart CREATE_PROCESS ipc_call failed");
        return false;
    };
    if reply_label != 0
    {
        runtime::log!("svcmgr: restart CREATE_PROCESS failed");
        return false;
    }

    // SAFETY: ipc_buf is the registered IPC buffer.
    let pid = unsafe { core::ptr::read_volatile(ipc_buf) };
    // SAFETY: ipc_buf is the registered IPC buffer, page-aligned.
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc_buf.cast::<u64>()) };
    if cap_count < 3
    {
        runtime::log!("svcmgr: restart reply missing caps");
        return false;
    }
    let child_cspace = reply_caps[0];
    let pi_frame = reply_caps[1];
    let new_thread_cap = reply_caps[2];

    // 2. Inject log endpoint into child CSpace.
    let inject_log = if svc.log_ep_cap != 0
    {
        svc.log_ep_cap
    }
    else
    {
        log_ep
    };

    let mut descs = [CapDescriptor {
        slot: 0,
        cap_type: CapType::Frame,
        pad: [0; 3],
        aux0: 0,
        aux1: 0,
    }; 4];
    let mut desc_count: usize = 0;
    let mut first_slot: u32 = 0;

    if inject_log != 0
    {
        if let Ok(derived) = syscall::cap_derive(inject_log, !0u64)
        {
            if let Ok(child_slot) = syscall::cap_copy(derived, child_cspace, !0u64)
            {
                first_slot = child_slot;
                descs[0] = CapDescriptor {
                    slot: child_slot,
                    cap_type: CapType::Frame,
                    pad: [0; 3],
                    aux0: LOG_ENDPOINT_SENTINEL,
                    aux1: 0,
                };
                desc_count = 1;
            }
        }
    }

    // 3. Patch ProcessInfo with CapDescriptors.
    if syscall::mem_map(
        pi_frame,
        self_aspace,
        CHILD_PI_VA,
        0,
        1,
        syscall::PROT_WRITE,
    )
    .is_err()
    {
        runtime::log!("svcmgr: cannot map child ProcessInfo");
        return false;
    }

    // SAFETY: CHILD_PI_VA is mapped writable to the ProcessInfo page.
    // cast_ptr_alignment: CHILD_PI_VA is page-aligned (4096-byte).
    #[allow(clippy::cast_ptr_alignment)]
    let pi = unsafe { &mut *(CHILD_PI_VA as *mut ProcessInfo) };

    if desc_count > 0
    {
        pi.initial_caps_base = first_slot;
        pi.initial_caps_count = desc_count as u32;
        pi.cap_descriptor_count = desc_count as u32;

        let descs_offset = core::mem::size_of::<ProcessInfo>() as u32;
        let descs_offset_aligned = (descs_offset + 7) & !7;
        pi.cap_descriptors_offset = descs_offset_aligned;

        let desc_size = core::mem::size_of::<CapDescriptor>();
        for (i, desc) in descs.iter().enumerate().take(desc_count)
        {
            let byte_offset = descs_offset_aligned as usize + i * desc_size;
            if byte_offset + desc_size > PAGE_SIZE as usize
            {
                break;
            }
            // SAFETY: byte_offset is within the mapped page; descs_offset_aligned
            // is 8-byte aligned and CapDescriptor is 24 bytes.
            #[allow(clippy::cast_ptr_alignment)]
            unsafe {
                let ptr = (CHILD_PI_VA as *mut u8)
                    .add(byte_offset)
                    .cast::<CapDescriptor>();
                core::ptr::write(ptr, *desc);
            }
        }
    }

    let _ = syscall::mem_unmap(self_aspace, CHILD_PI_VA, 1);

    // 4. START_PROCESS.
    // SAFETY: writing pid to IPC buffer for START_PROCESS.
    unsafe { core::ptr::write_volatile(ipc_buf, pid) };
    if !matches!(
        syscall::ipc_call(procmgr_ep, LABEL_START_PROCESS, 1, &[]),
        Ok((0, _))
    )
    {
        runtime::log!("svcmgr: restart START_PROCESS failed");
        return false;
    }

    // 5. Rebind death notification: new event queue on the new thread.
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

fn halt_loop() -> !
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

// ── Entry point ─────────────────────────────────────────────────────────────

#[allow(clippy::too_many_lines)]
#[no_mangle]
extern "Rust" fn main(startup: &StartupInfo) -> !
{
    // Register IPC buffer.
    if syscall::ipc_buffer_set(startup.ipc_buffer as u64).is_err()
    {
        syscall::thread_exit();
    }

    let caps = classify_caps(startup);
    // cast_ptr_alignment: IPC buffer is page-aligned (4096-byte), satisfying u64 alignment.
    #[allow(clippy::cast_ptr_alignment)]
    let ipc_buf = startup.ipc_buffer.cast::<u64>();

    if caps.log_ep != 0
    {
        // SAFETY: single-threaded; called once before any log calls.
        unsafe { runtime::log::log_init(caps.log_ep, startup.ipc_buffer) };
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
            // IPC on service endpoint — registration or handover.
            let Ok((label, _data_count)) = syscall::ipc_recv(caps.service_ep)
            else
            {
                continue;
            };

            let opcode = label & 0xFFFF;
            match opcode
            {
                LABEL_REGISTER =>
                {
                    let result =
                        handle_register(ipc_buf, label, &mut services, &mut service_count, ws_cap);
                    let _ = syscall::ipc_reply(result, 0, &[]);
                }
                LABEL_HANDOVER =>
                {
                    handover_complete = true;
                    let _ = syscall::ipc_reply(0, 0, &[]);
                    runtime::log!(
                        "svcmgr: handover complete, monitoring services: {:#018x}",
                        service_count as u64
                    );
                }
                _ =>
                {
                    let _ = syscall::ipc_reply(0xFFFF, 0, &[]);
                }
            }
        }
        else
        {
            // Death notification from service at index (token - 1).
            let idx = (token - 1) as usize;
            if idx >= service_count
            {
                runtime::log!("svcmgr: invalid death notification token");
                continue;
            }

            let Ok(exit_reason) = syscall::event_recv(services[idx].event_queue_cap)
            else
            {
                runtime::log!("svcmgr: event_recv failed");
                continue;
            };

            if !services[idx].active
            {
                continue;
            }

            handle_death(
                &mut services[idx],
                exit_reason,
                caps.procmgr_ep,
                caps.self_aspace,
                caps.log_ep,
                ws_cap,
                ipc_buf,
            );

            // Re-add event queue to wait set with same token (if still active).
            if services[idx].active
                && syscall::wait_set_add(ws_cap, services[idx].event_queue_cap, token).is_err()
            {
                runtime::log!("svcmgr: failed to re-add event queue to wait set after restart");
                services[idx].active = false;
            }

            let _ = handover_complete;
        }
    }
}
