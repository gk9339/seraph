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
mod worker;

use gpt::MAX_GPT_PARTS;
use ipc::{blk_labels, procmgr_labels, IpcBuf};
use mount::{MountEntry, MAX_MOUNTS};
use process_abi::StartupInfo;

/// Monotonic counter for per-partition block endpoint tokens.
///
/// Each tokened cap derived from the whole-disk block endpoint gets a fresh
/// non-zero token; virtio-blk's partition table keys on this value. Token 0
/// is reserved for the un-tokened (whole-disk) endpoint held by vfsd.
static NEXT_PARTITION_TOKEN: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(1);

// ── Data structures ────────────────────────────────────────────────────────

pub struct VfsdCaps
{
    pub log_ep: u32,
    pub procmgr_ep: u32,
    pub registry_ep: u32,
    pub service_ep: u32,
    pub fatfs_module_cap: u32,
    pub self_aspace: u32,
    pub self_cspace: u32,
    pub bootstrap_ep: u32,
    pub done_sig: u32,
}

// ── Bootstrap ──────────────────────────────────────────────────────────────
//
// init → vfsd bootstrap plan (one round, 4 caps, 0 data words):
//   caps[0]: log endpoint
//   caps[1]: service endpoint (vfsd receives on this)
//   caps[2]: devmgr registry endpoint
//   caps[3]: procmgr service endpoint
// Round 2 (1 cap, 0 data words):
//   caps[0]: fatfs module frame cap

fn bootstrap_caps(startup: &StartupInfo, ipc: IpcBuf) -> Option<VfsdCaps>
{
    if startup.creator_endpoint == 0
    {
        return None;
    }
    let round1 = ipc::bootstrap::request_round(startup.creator_endpoint, ipc).ok()?;
    if round1.cap_count < 4 || round1.done
    {
        return None;
    }
    let round2 = ipc::bootstrap::request_round(startup.creator_endpoint, ipc).ok()?;
    if round2.cap_count < 1 || !round2.done
    {
        return None;
    }

    Some(VfsdCaps {
        log_ep: round1.caps[0],
        service_ep: round1.caps[1],
        registry_ep: round1.caps[2],
        procmgr_ep: round1.caps[3],
        fatfs_module_cap: round2.caps[0],
        self_aspace: startup.self_aspace,
        self_cspace: startup.self_cspace,
        bootstrap_ep: 0,
        done_sig: 0,
    })
}

// ── Worker thread setup ────────────────────────────────────────────────────

/// Request a single-page frame from procmgr and return the cap slot.
fn request_page(procmgr_ep: u32, ipc: IpcBuf) -> Option<u32>
{
    ipc.write_word(0, 1);
    let Ok((label, _)) = syscall::ipc_call(procmgr_ep, procmgr_labels::REQUEST_FRAMES, 1, &[])
    else
    {
        return None;
    };
    if label != 0
    {
        return None;
    }
    // SAFETY: ipc wraps the registered IPC buffer; kernel just wrote cap metadata.
    let (cap_count, slots) = unsafe { syscall::read_recv_caps(ipc.as_ptr()) };
    if cap_count == 0
    {
        return None;
    }
    Some(slots[0])
}

/// Allocate pages for the worker, map stack + IPC buffer, create the bootstrap
/// endpoint and done signal, and start the worker thread. Returns
/// `(bootstrap_ep, done_sig)` on success.
fn spawn_worker(caps: &VfsdCaps, ipc: IpcBuf) -> Option<(u32, u32)>
{
    // Allocate and map the worker's stack pages.
    for i in 0..worker::STACK_PAGES
    {
        let frame = request_page(caps.procmgr_ep, ipc)?;
        let rw = syscall::cap_derive(frame, syscall::RIGHTS_MAP_RW).ok()?;
        syscall::mem_map(
            rw,
            caps.self_aspace,
            worker::STACK_BASE + i * 4096,
            0,
            1,
            syscall::MAP_WRITABLE,
        )
        .ok()?;
    }

    // Allocate and map the worker's IPC buffer.
    let ipc_frame = request_page(caps.procmgr_ep, ipc)?;
    let ipc_rw = syscall::cap_derive(ipc_frame, syscall::RIGHTS_MAP_RW).ok()?;
    syscall::mem_map(
        ipc_rw,
        caps.self_aspace,
        worker::IPC_BUF_VA,
        0,
        1,
        syscall::MAP_WRITABLE,
    )
    .ok()?;
    // Zero the IPC buffer.
    // SAFETY: IPC_BUF_VA mapped writable, one page.
    unsafe {
        core::ptr::write_bytes(worker::IPC_BUF_VA as *mut u8, 0, 4096);
    }

    let bootstrap_ep = syscall::cap_create_endpoint().ok()?;
    let done_sig = syscall::cap_create_signal().ok()?;

    worker::BOOTSTRAP_EP.store(bootstrap_ep, core::sync::atomic::Ordering::Release);
    worker::DONE_SIG.store(done_sig, core::sync::atomic::Ordering::Release);

    let thread_cap = syscall::cap_create_thread(caps.self_aspace, caps.self_cspace).ok()?;

    syscall::thread_configure(
        thread_cap,
        worker::entry as *const () as u64,
        worker::STACK_TOP,
        0,
    )
    .ok()?;
    syscall::thread_start(thread_cap).ok()?;

    Some((bootstrap_ep, done_sig))
}

// ── Entry point ────────────────────────────────────────────────────────────

#[no_mangle]
extern "Rust" fn main(startup: &StartupInfo) -> !
{
    let _ = syscall::ipc_buffer_set(startup.ipc_buffer as u64);

    // SAFETY: IPC buffer is registered and page-aligned.
    let ipc = unsafe { IpcBuf::from_bytes(startup.ipc_buffer) };
    let ipc_buf = ipc.as_ptr();

    let Some(mut caps) = bootstrap_caps(startup, ipc)
    else
    {
        syscall::thread_exit();
    };

    // Initialise IPC logging.
    if caps.log_ep != 0
    {
        runtime::log::log_init(caps.log_ep, startup.ipc_buffer);
    }

    runtime::log!("vfsd: starting");

    if caps.service_ep == 0 || caps.registry_ep == 0
    {
        runtime::log!("vfsd: missing required endpoints");
        idle_loop();
    }

    // Spawn the bootstrap worker thread before any MOUNT can arrive.
    let Some((bootstrap_ep, done_sig)) = spawn_worker(&caps, ipc)
    else
    {
        runtime::log!("vfsd: FATAL: worker thread setup failed");
        idle_loop();
    };
    caps.bootstrap_ep = bootstrap_ep;
    caps.done_sig = done_sig;

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
    match gpt::parse_gpt(blk_ep, ipc_buf, &mut gpt_parts)
    {
        Ok(count) => runtime::log!("vfsd: GPT parsed, {} partitions", count),
        Err(gpt::GptError::IoError) => runtime::log!("vfsd: GPT parse failed: I/O error"),
        Err(gpt::GptError::InvalidSignature) =>
        {
            runtime::log!("vfsd: GPT parse failed: invalid signature");
        }
        Err(gpt::GptError::InvalidEntrySize) =>
        {
            runtime::log!("vfsd: GPT parse failed: invalid entry size");
        }
    }

    runtime::log!("vfsd: entering service loop");
    let runtime = VfsdRuntime {
        caps: &caps,
        blk_ep,
        gpt_parts: &gpt_parts,
    };
    service_loop(ipc_buf, &runtime);
}

/// Live references the service loop and its handlers need on every request.
pub struct VfsdRuntime<'a>
{
    pub caps: &'a VfsdCaps,
    pub blk_ep: u32,
    pub gpt_parts: &'a [gpt::GptEntry; MAX_GPT_PARTS],
}

// ── Service loop ───────────────────────────────────────────────────────────

/// Main VFS service loop — namespace resolution and mount management.
///
/// Handles `OPEN` (resolves mount point, forwards to driver, relays per-file
/// capability to client) and `MOUNT` requests. Clients perform file operations
/// (read/close/stat/readdir) directly on the per-file capability returned by
/// `OPEN`, without further vfsd involvement.
fn service_loop(ipc_buf: *mut u64, rt: &VfsdRuntime) -> !
{
    let mut mounts = mount::new_mount_table();

    loop
    {
        let Ok((label, _token)) = syscall::ipc_recv(rt.caps.service_ep)
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
                handle_mount_request(ipc_buf, rt, &mut mounts);
            }
            _ =>
            {
                let _ = syscall::ipc_reply(ipc::vfsd_errors::UNKNOWN_OPCODE, 0, &[]);
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
fn handle_mount_request(ipc_buf: *mut u64, rt: &VfsdRuntime, mounts: &mut [MountEntry; MAX_MOUNTS])
{
    // SAFETY: ipc_buf is the registered IPC buffer page for vfsd.
    let ipc = unsafe { ipc::IpcBuf::from_raw(ipc_buf) };
    let w0 = ipc.read_word(0);
    let w1 = ipc.read_word(1);
    let mut uuid = [0u8; 16];
    uuid[..8].copy_from_slice(&w0.to_le_bytes());
    uuid[8..].copy_from_slice(&w1.to_le_bytes());

    let path_len = ipc.read_word(2) as usize;
    if path_len == 0 || path_len > 64
    {
        runtime::log!("vfsd: MOUNT: invalid path length");
        let _ = syscall::ipc_reply(ipc::vfsd_errors::NOT_FOUND, 0, &[]);
        return;
    }

    let mut path_buf = [0u8; 64];
    let word_count = path_len.div_ceil(8).min(8);
    for i in 0..word_count
    {
        let word = ipc.read_word(3 + i);
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
    let Some((partition_lba, partition_len)) = gpt::lookup_partition_by_uuid(&uuid, rt.gpt_parts)
    else
    {
        runtime::log!("vfsd: MOUNT: partition UUID not found");
        let _ = syscall::ipc_reply(ipc::vfsd_errors::NO_MOUNT, 0, &[]);
        return;
    };
    runtime::log!(
        "vfsd: MOUNT: partition LBA={:#018x} length={:#018x}",
        partition_lba,
        partition_len
    );

    // Spawn fatfs driver for this partition.
    if rt.caps.fatfs_module_cap == 0
    {
        runtime::log!("vfsd: MOUNT: no fatfs module cap");
        let _ = syscall::ipc_reply(ipc::vfsd_errors::NO_FS_MODULE, 0, &[]);
        return;
    }

    // Derive a partition-scoped tokened SEND cap on the whole-disk block
    // endpoint, and register its bound with virtio-blk. fatfs will only
    // ever see this tokened cap; virtio-blk enforces bounds per token.
    let Some(partition_ep) =
        derive_and_register_partition(rt, partition_lba, partition_len, ipc_buf)
    else
    {
        runtime::log!("vfsd: MOUNT: partition cap registration failed");
        let _ = syscall::ipc_reply(ipc::vfsd_errors::SPAWN_FAILED, 0, &[]);
        return;
    };

    // SAFETY: ipc_buf wraps the registered IPC page.
    let ipc_wrap = unsafe { IpcBuf::from_raw(ipc_buf) };
    let Some(driver_ep) = driver::spawn_fatfs_driver(rt.caps, partition_ep, ipc_wrap)
    else
    {
        runtime::log!("vfsd: MOUNT: failed to spawn fatfs");
        let _ = syscall::ipc_reply(ipc::vfsd_errors::SPAWN_FAILED, 0, &[]);
        return;
    };

    // Register mount entry.
    if mount::register_mount(mounts, &path_buf, path_len, driver_ep)
    {
        let _ = syscall::ipc_reply(ipc::vfsd_errors::SUCCESS, 0, &[]);
        runtime::log!("vfsd: MOUNT: registered");
    }
    else
    {
        let _ = syscall::ipc_reply(ipc::vfsd_errors::TABLE_FULL, 0, &[]);
        runtime::log!("vfsd: MOUNT: mount table full");
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
        let _ = syscall::ipc_reply(ipc::vfsd_errors::NOT_FOUND, 0, &[]);
        return;
    }

    let mut path_buf = [0u8; ipc::MAX_PATH_LEN];
    // SAFETY: ipc_buf is the registered IPC buffer page.
    let ipc_ref = unsafe { ipc::IpcBuf::from_raw(ipc_buf) };
    let _ = ipc::read_path_from_ipc(ipc_ref, path_len, &mut path_buf);
    let path = &path_buf[..path_len];

    let Some((mount_idx, driver_path)) = mount::resolve_mount(path, mounts)
    else
    {
        let _ = syscall::ipc_reply(ipc::vfsd_errors::NO_MOUNT, 0, &[]);
        return;
    };

    let driver_ep = mounts[mount_idx].driver_ep;

    // Forward FS_OPEN to driver with the driver-relative path.
    let fwd_path_len = driver_path.len();
    let _ = ipc::write_path_to_ipc(ipc_ref, driver_path);

    let fwd_label = ipc::fs_labels::FS_OPEN | ((fwd_path_len as u64) << 16);
    let data_words = fwd_path_len.div_ceil(8).min(6);
    let Ok((drv_reply, _)) = syscall::ipc_call(driver_ep, fwd_label, data_words, &[])
    else
    {
        let _ = syscall::ipc_reply(ipc::vfsd_errors::IO_ERROR, 0, &[]);
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
        let _ = syscall::ipc_reply(ipc::vfsd_errors::IO_ERROR, 0, &[]);
        return;
    }

    // Relay the file cap to the client.
    let _ = syscall::ipc_reply(ipc::vfsd_errors::SUCCESS, 0, &[reply_caps[0]]);
}

fn idle_loop() -> !
{
    loop
    {
        let _ = syscall::thread_yield();
    }
}

/// Derive a per-partition tokened SEND cap on the whole-disk block endpoint
/// and register the partition bound with virtio-blk. Returns the tokened cap
/// slot on success.
fn derive_and_register_partition(
    rt: &VfsdRuntime,
    base_lba: u64,
    length_lba: u64,
    ipc_buf: *mut u64,
) -> Option<u32>
{
    let token = NEXT_PARTITION_TOKEN.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    let partition_ep = syscall::cap_derive_token(rt.blk_ep, syscall::RIGHTS_SEND, token).ok()?;

    // REGISTER_PARTITION on the un-tokened (whole-disk) endpoint.
    // SAFETY: ipc_buf is the registered IPC buffer.
    let ipc = unsafe { ipc::IpcBuf::from_raw(ipc_buf) };
    ipc.write_word(0, token);
    ipc.write_word(1, base_lba);
    ipc.write_word(2, length_lba);
    let Ok((reply, _)) = syscall::ipc_call(rt.blk_ep, blk_labels::REGISTER_PARTITION, 3, &[])
    else
    {
        runtime::log!("vfsd: REGISTER_PARTITION ipc_call failed");
        return None;
    };
    if reply != ipc::blk_errors::SUCCESS
    {
        runtime::log!("vfsd: REGISTER_PARTITION rejected (code={})", reply);
        return None;
    }
    Some(partition_ep)
}
