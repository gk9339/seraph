// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// vfsd/src/main.rs

//! Seraph virtual filesystem daemon.
//!
//! vfsd presents a unified virtual filesystem namespace to all other processes.
//! It manages filesystem driver instances and routes `OPEN` requests to the
//! appropriate backing driver based on mount-point resolution. After opening,
//! clients hold a direct per-file capability to the driver and perform
//! read/close/stat/readdir operations without vfsd involvement.
//!
//! See `vfsd/README.md` for the design, `vfsd/docs/vfs-ipc-interface.md` for
//! the client-facing IPC protocol, and `fs/docs/fs-driver-protocol.md` for the
//! driver-side protocol.

#![no_std]
#![no_main]
#![allow(clippy::cast_possible_truncation)]

extern crate runtime;

mod driver;
mod gpt;
mod mount;

use gpt::MAX_GPT_PARTS;
use mount::{MountEntry, MAX_MOUNTS};
use process_abi::{CapType, StartupInfo};

// ── Data structures ────────────────────────────────────────────────────────

pub struct VfsdCaps
{
    pub log_ep: u32,
    pub procmgr_ep: u32,
    pub registry_ep: u32,
    pub service_ep: u32,
    pub fatfs_module_cap: u32,
    pub self_aspace: u32,
}

// ── Cap classification ─────────────────────────────────────────────────────

fn classify_caps(startup: &StartupInfo) -> VfsdCaps
{
    let mut caps = VfsdCaps {
        log_ep: 0,
        procmgr_ep: 0,
        registry_ep: 0,
        service_ep: 0,
        fatfs_module_cap: 0,
        self_aspace: startup.self_aspace,
    };

    for d in startup.initial_caps
    {
        if d.cap_type == CapType::Frame
        {
            if d.aux0 == ipc::LOG_ENDPOINT_SENTINEL
            {
                caps.log_ep = d.slot;
            }
            else if d.aux0 == ipc::SERVICE_ENDPOINT_SENTINEL
            {
                caps.service_ep = d.slot;
            }
            else if d.aux0 == ipc::REGISTRY_ENDPOINT_SENTINEL
            {
                caps.registry_ep = d.slot;
            }
            else if d.aux0 == 0 && d.aux1 == 0
            {
                // procmgr endpoint sentinel.
                caps.procmgr_ep = d.slot;
            }
            else if d.aux0 == 4
            {
                // fatfs module frame (module index 4).
                caps.fatfs_module_cap = d.slot;
            }
        }
    }

    caps
}

// ── Entry point ────────────────────────────────────────────────────────────

#[no_mangle]
extern "Rust" fn main(startup: &StartupInfo) -> !
{
    let _ = syscall::ipc_buffer_set(startup.ipc_buffer as u64);

    let caps = classify_caps(startup);

    // Initialise IPC logging.
    if caps.log_ep != 0
    {
        // SAFETY: single-threaded; called once before any log calls.
        unsafe { runtime::log::log_init(caps.log_ep, startup.ipc_buffer) };
    }

    runtime::log!("vfsd: starting");

    // SAFETY: IPC buffer is page-aligned.
    #[allow(clippy::cast_ptr_alignment)]
    let ipc_buf = startup.ipc_buffer.cast::<u64>();

    if caps.service_ep == 0 || caps.registry_ep == 0
    {
        runtime::log!("vfsd: missing required endpoints");
        idle_loop();
    }

    // Query devmgr for the block device endpoint.
    runtime::log!("vfsd: querying devmgr for block device");
    let Ok((reply_label, _)) = syscall::ipc_call(
        caps.registry_ep,
        ipc::devmgr_labels::QUERY_BLOCK_DEVICE,
        0,
        &[],
    )
    else
    {
        runtime::log!("vfsd: QUERY_BLOCK_DEVICE ipc_call failed");
        idle_loop();
    };
    if reply_label != 0
    {
        runtime::log!("vfsd: no block device available");
        idle_loop();
    }

    // SAFETY: ipc_buf is the registered IPC buffer.
    #[allow(clippy::cast_ptr_alignment)]
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc_buf) };
    if cap_count == 0
    {
        runtime::log!("vfsd: QUERY_BLOCK_DEVICE returned no caps");
        idle_loop();
    }
    let blk_ep = reply_caps[0];
    runtime::log!("vfsd: block device endpoint acquired");

    // Parse GPT partition table — stored for UUID lookups on MOUNT requests.
    let mut gpt_parts = gpt::new_gpt_table();
    let _gpt_count = gpt::parse_gpt(blk_ep, ipc_buf, &mut gpt_parts);

    runtime::log!("vfsd: entering service loop");
    service_loop(caps.service_ep, ipc_buf, &caps, blk_ep, &gpt_parts);
}

// ── Service loop ───────────────────────────────────────────────────────────

/// Main VFS service loop — namespace resolution and mount management.
///
/// Handles `OPEN` (resolves mount point, forwards to driver, relays per-file
/// capability to client) and `MOUNT` requests. Clients perform file operations
/// (read/close/stat/readdir) directly on the per-file capability returned by
/// `OPEN`, without further vfsd involvement.
#[allow(clippy::too_many_arguments)]
fn service_loop(
    service_ep: u32,
    ipc_buf: *mut u64,
    caps: &VfsdCaps,
    blk_ep: u32,
    gpt_parts: &[gpt::GptEntry; MAX_GPT_PARTS],
) -> !
{
    let mut mounts = mount::new_mount_table();

    loop
    {
        let Ok((label, _token)) = syscall::ipc_recv(service_ep)
        else
        {
            runtime::log!("vfsd: ipc_recv failed, retrying");
            continue;
        };

        let opcode = label & 0xFFFF;

        match opcode
        {
            ipc::vfsd_labels::OPEN => handle_open(label, ipc_buf, &mounts),
            ipc::vfsd_labels::MOUNT =>
            {
                handle_mount_request(ipc_buf, caps, blk_ep, gpt_parts, &mut mounts);
            }
            _ =>
            {
                let _ = syscall::ipc_reply(0xFF, 0, &[]);
            }
        }
    }
}

// ── MOUNT handler ──────────────────────────────────────────────────────────

/// Handle a MOUNT request from init (or any authorized client).
///
/// IPC data layout:
/// - `data[0..2]`: partition UUID (16 bytes, mixed-endian, as stored in GPT)
/// - `data[2]`: mount path length
/// - `data[3..]`: mount path bytes (packed into u64 words)
///
/// Looks up the UUID in the GPT table, spawns a fatfs driver with the
/// partition's LBA offset, and registers a mount entry at the given path.
#[allow(clippy::too_many_arguments)]
fn handle_mount_request(
    ipc_buf: *mut u64,
    caps: &VfsdCaps,
    blk_ep: u32,
    gpt_parts: &[gpt::GptEntry; MAX_GPT_PARTS],
    mounts: &mut [MountEntry; MAX_MOUNTS],
)
{
    // SAFETY: IPC buffer is valid and word-aligned.
    let w0 = unsafe { core::ptr::read_volatile(ipc_buf) };
    // SAFETY: see above.
    let w1 = unsafe { core::ptr::read_volatile(ipc_buf.add(1)) };
    let mut uuid = [0u8; 16];
    uuid[..8].copy_from_slice(&w0.to_le_bytes());
    uuid[8..].copy_from_slice(&w1.to_le_bytes());

    // SAFETY: IPC buffer is valid.
    let path_len = unsafe { core::ptr::read_volatile(ipc_buf.add(2)) } as usize;
    if path_len == 0 || path_len > 64
    {
        runtime::log!("vfsd: MOUNT: invalid path length");
        let _ = syscall::ipc_reply(1, 0, &[]);
        return;
    }

    let mut path_buf = [0u8; 64];
    let word_count = path_len.div_ceil(8).min(8);
    for i in 0..word_count
    {
        // SAFETY: IPC buffer is valid.
        let word = unsafe { core::ptr::read_volatile(ipc_buf.add(3 + i)) };
        let base = i * 8;
        let bytes = word.to_le_bytes();
        for j in 0..8
        {
            if base + j < path_len
            {
                path_buf[base + j] = bytes[j];
            }
        }
    }

    // Look up UUID in GPT partition table.
    let partition_lba = gpt::lookup_partition_by_uuid(&uuid, gpt_parts);
    if partition_lba == 0
    {
        runtime::log!("vfsd: MOUNT: partition UUID not found");
        let _ = syscall::ipc_reply(2, 0, &[]);
        return;
    }
    runtime::log!("vfsd: MOUNT: partition LBA={:#018x}", partition_lba);

    // Spawn fatfs driver for this partition.
    if caps.fatfs_module_cap == 0
    {
        runtime::log!("vfsd: MOUNT: no fatfs module cap");
        let _ = syscall::ipc_reply(3, 0, &[]);
        return;
    }

    let Some(driver_ep) = driver::spawn_fatfs_driver(caps, blk_ep, ipc_buf)
    else
    {
        runtime::log!("vfsd: MOUNT: failed to spawn fatfs");
        let _ = syscall::ipc_reply(4, 0, &[]);
        return;
    };

    // Send FS_MOUNT to the fatfs driver with the partition LBA offset.
    // SAFETY: IPC buffer is valid.
    unsafe { core::ptr::write_volatile(ipc_buf, partition_lba) };
    if let Ok((0, _)) = syscall::ipc_call(driver_ep, ipc::fs_labels::FS_MOUNT, 1, &[])
    {
        runtime::log!("vfsd: MOUNT: fatfs mounted successfully");
    }
    else
    {
        runtime::log!("vfsd: MOUNT: fatfs FS_MOUNT failed");
        let _ = syscall::ipc_reply(5, 0, &[]);
        return;
    }

    // Register mount entry.
    if mount::register_mount(mounts, &path_buf, path_len, driver_ep)
    {
        runtime::log!("vfsd: MOUNT: registered");
        let _ = syscall::ipc_reply(0, 0, &[]);
    }
    else
    {
        runtime::log!("vfsd: MOUNT: mount table full");
        let _ = syscall::ipc_reply(6, 0, &[]);
    }
}

// ── OPEN handler ──────────────────────────────────────────────────────────

/// Handle an OPEN request: resolve the mount point, forward `FS_OPEN` to the
/// driver, and relay the per-file capability back to the client.
///
/// After this call, the client holds a direct tokened capability to the fs
/// driver for file operations (read/close/stat/readdir).
fn handle_open(label: u64, ipc_buf: *mut u64, mounts: &[MountEntry; MAX_MOUNTS])
{
    let path_len = ((label >> 16) & 0xFFFF) as usize;
    if path_len == 0 || path_len > ipc::MAX_PATH_LEN
    {
        let _ = syscall::ipc_reply(1, 0, &[]); // NotFound
        return;
    }

    let mut path_buf = [0u8; ipc::MAX_PATH_LEN];
    // SAFETY: ipc_buf is valid and has path data words.
    unsafe { ipc::read_path_from_ipc(ipc_buf, path_len, &mut path_buf) };
    let path = &path_buf[..path_len];

    let Some((mount_idx, driver_path)) = mount::resolve_mount(path, mounts)
    else
    {
        let _ = syscall::ipc_reply(2, 0, &[]); // NoMount
        return;
    };

    let driver_ep = mounts[mount_idx].driver_ep;

    // Forward FS_OPEN to driver with the driver-relative path.
    let fwd_path_len = driver_path.len();
    // SAFETY: ipc_buf is valid.
    unsafe { ipc::write_path_to_ipc(ipc_buf, driver_path) };

    let fwd_label = ipc::fs_labels::FS_OPEN | ((fwd_path_len as u64) << 16);
    let data_words = fwd_path_len.div_ceil(8).min(6);
    let Ok((drv_reply, _)) = syscall::ipc_call(driver_ep, fwd_label, data_words, &[])
    else
    {
        let _ = syscall::ipc_reply(5, 0, &[]); // IoError
        return;
    };

    if drv_reply != 0
    {
        // Driver returned an error — relay it to client.
        let _ = syscall::ipc_reply(drv_reply, 0, &[]);
        return;
    }

    // Read the per-file capability from the driver's reply.
    // SAFETY: ipc_buf is the registered IPC buffer.
    #[allow(clippy::cast_ptr_alignment)]
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc_buf) };
    if cap_count == 0
    {
        runtime::log!("vfsd: OPEN: driver returned no file cap");
        let _ = syscall::ipc_reply(5, 0, &[]); // IoError
        return;
    }

    // Relay the file cap to the client.
    let _ = syscall::ipc_reply(0, 0, &[reply_caps[0]]);
}

fn idle_loop() -> !
{
    loop
    {
        let _ = syscall::thread_yield();
    }
}
