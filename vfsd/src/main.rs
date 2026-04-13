// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// vfsd/src/main.rs

//! Seraph virtual filesystem daemon.
//!
//! vfsd presents a unified virtual filesystem namespace to all other processes.
//! It manages filesystem driver instances and routes VFS IPC requests to the
//! appropriate backing driver.
//!
//! See `vfsd/README.md` for the design, `vfsd/docs/vfs-ipc-interface.md` for
//! the client-facing IPC protocol, and `fs/docs/fs-driver-protocol.md` for the
//! driver-side protocol.

#![no_std]
#![no_main]
#![allow(clippy::cast_possible_truncation)]

extern crate runtime;

use process_abi::{CapDescriptor, CapType, ProcessInfo, StartupInfo};
use runtime::log::log;

// ── Constants ────────────────────────────────────────────────────────────────

const PAGE_SIZE: u64 = 0x1000;

/// VA for mapping child `ProcessInfo` frames during cap injection.
const CHILD_PI_VA: u64 = 0x0000_0002_0000_0000; // 8 GiB

// IPC label constants — client-facing (vfs-ipc-interface.md).
const LABEL_OPEN: u64 = 1;
const LABEL_READ: u64 = 2;
const LABEL_CLOSE: u64 = 3;
const LABEL_STAT: u64 = 4;
const LABEL_READDIR: u64 = 5;

// IPC label constants — fs-driver-protocol.
const LABEL_FS_MOUNT: u64 = 10;

// IPC labels for procmgr.
const LABEL_CREATE_PROCESS: u64 = 1;
const LABEL_START_PROCESS: u64 = 2;

// IPC label for devmgr registry.
const LABEL_QUERY_BLOCK_DEVICE: u64 = 1;

// Sentinel values for cap identification.
const LOG_ENDPOINT_SENTINEL: u64 = 0xFFFF_FFFF_FFFF_FFFF;
const SERVICE_ENDPOINT_SENTINEL: u64 = 0xFFFF_FFFF_FFFF_FFFE;
const REGISTRY_ENDPOINT_SENTINEL: u64 = 0xFFFF_FFFF_FFFF_FFFD;
const BLOCK_ENDPOINT_SENTINEL: u64 = 0xFFFF_FFFF_FFFF_FFFC;

/// Maximum mount table entries.
const MAX_MOUNTS: usize = 4;

/// Maximum open file descriptors.
const MAX_FDS: usize = 16;

/// Maximum cap descriptors when spawning a filesystem driver.
const MAX_DRIVER_DESCS: usize = 8;

/// Maximum GPT partitions we track.
const MAX_GPT_PARTS: usize = 8;

/// Sector size for block I/O.
const SECTOR_SIZE: usize = 512;

// ── Data structures ─────────────────────────────────────────────────────────

/// A discovered GPT partition (UUID + starting LBA).
struct GptEntry
{
    uuid: [u8; 16],
    first_lba: u64,
    active: bool,
}

impl GptEntry
{
    const fn empty() -> Self
    {
        Self {
            uuid: [0; 16],
            first_lba: 0,
            active: false,
        }
    }
}

struct MountEntry
{
    path: [u8; 64],
    path_len: usize,
    driver_ep: u32,
    active: bool,
}

impl MountEntry
{
    const fn empty() -> Self
    {
        Self {
            path: [0; 64],
            path_len: 0,
            driver_ep: 0,
            active: false,
        }
    }
}

struct FdEntry
{
    mount_idx: usize,
    driver_fd: u64,
    in_use: bool,
}

impl FdEntry
{
    const fn empty() -> Self
    {
        Self {
            mount_idx: 0,
            driver_fd: 0,
            in_use: false,
        }
    }
}

struct VfsdCaps
{
    log_ep: u32,
    procmgr_ep: u32,
    registry_ep: u32,
    service_ep: u32,
    fatfs_module_cap: u32,
    self_aspace: u32,
}

// ── Cap classification ──────────────────────────────────────────────────────

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
            if d.aux0 == LOG_ENDPOINT_SENTINEL
            {
                caps.log_ep = d.slot;
            }
            else if d.aux0 == SERVICE_ENDPOINT_SENTINEL
            {
                caps.service_ep = d.slot;
            }
            else if d.aux0 == REGISTRY_ENDPOINT_SENTINEL
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

// ── Mount table and fd table ────────────────────────────────────────────────

/// Resolve a path to the mount entry with the longest matching prefix.
///
/// Returns the mount index and the driver-relative path (after stripping
/// the mount prefix).
fn resolve_mount<'a>(path: &'a [u8], mounts: &[MountEntry; MAX_MOUNTS])
    -> Option<(usize, &'a [u8])>
{
    let mut best_idx = None;
    let mut best_len = 0;

    for (i, m) in mounts.iter().enumerate()
    {
        if !m.active
        {
            continue;
        }
        // Check prefix match and keep longest.
        if path.len() >= m.path_len
            && path[..m.path_len] == m.path[..m.path_len]
            && m.path_len > best_len
        {
            best_len = m.path_len;
            best_idx = Some(i);
        }
    }

    best_idx.map(|i| {
        let rest = if best_len < path.len()
        {
            &path[best_len..]
        }
        else
        {
            &[] as &[u8]
        };
        (i, rest)
    })
}

fn alloc_fd(fds: &mut [FdEntry; MAX_FDS], mount_idx: usize, driver_fd: u64) -> Option<u64>
{
    for (i, fd) in fds.iter_mut().enumerate()
    {
        if !fd.in_use
        {
            fd.in_use = true;
            fd.mount_idx = mount_idx;
            fd.driver_fd = driver_fd;
            return Some(i as u64);
        }
    }
    None
}

fn free_fd(fds: &mut [FdEntry; MAX_FDS], fd_idx: u64) -> bool
{
    let idx = fd_idx as usize;
    if idx < MAX_FDS && fds[idx].in_use
    {
        fds[idx].in_use = false;
        true
    }
    else
    {
        false
    }
}

// ── Two-phase process creation ──────────────────────────────────────────────

/// Spawn the fatfs driver via procmgr with block device and log endpoint caps.
///
/// Returns the driver's IPC endpoint (send cap) on success.
// too_many_lines: two-phase creation is inherently sequential.
#[allow(clippy::too_many_lines)]
fn spawn_fatfs_driver(caps: &VfsdCaps, blk_ep: u32, ipc_buf: *mut u64) -> Option<u32>
{
    // Derive a copy of the fatfs module cap for this spawn. The original
    // is retained so additional fatfs instances can be created for other mounts.
    let Ok(module_copy) = syscall::cap_derive(caps.fatfs_module_cap, !0u64)
    else
    {
        log("vfsd: cannot derive fatfs module cap");
        return None;
    };

    // Phase 1: CREATE_PROCESS (suspended).
    let Ok((reply_label, _)) =
        syscall::ipc_call(caps.procmgr_ep, LABEL_CREATE_PROCESS, 0, &[module_copy])
    else
    {
        log("vfsd: fatfs CREATE_PROCESS ipc_call failed");
        return None;
    };
    if reply_label != 0
    {
        log("vfsd: fatfs CREATE_PROCESS failed");
        return None;
    }

    // SAFETY: IPC buffer is valid and kernel wrote reply data.
    let pid = unsafe { core::ptr::read_volatile(ipc_buf) };
    // SAFETY: ipc_buf is the registered IPC buffer.
    #[allow(clippy::cast_ptr_alignment)]
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc_buf) };
    if cap_count < 2
    {
        log("vfsd: fatfs CREATE_PROCESS reply missing caps");
        return None;
    }
    let child_cspace = reply_caps[0];
    let pi_frame = reply_caps[1];

    // Phase 2: Inject caps.
    let mut descs = [CapDescriptor {
        slot: 0,
        cap_type: CapType::Frame,
        pad: [0; 3],
        aux0: 0,
        aux1: 0,
    }; MAX_DRIVER_DESCS];
    let mut desc_count: usize = 0;
    let mut first_slot: u32 = 0;

    // Create driver endpoint for vfsd-to-driver IPC.
    let Ok(driver_ep) = syscall::cap_create_endpoint()
    else
    {
        log("vfsd: failed to create fatfs driver endpoint");
        return None;
    };

    // Inject block device endpoint (send cap).
    inject_cap(
        blk_ep,
        BLOCK_ENDPOINT_SENTINEL,
        child_cspace,
        &mut descs,
        &mut desc_count,
        &mut first_slot,
    );

    // Inject log endpoint.
    inject_cap(
        caps.log_ep,
        LOG_ENDPOINT_SENTINEL,
        child_cspace,
        &mut descs,
        &mut desc_count,
        &mut first_slot,
    );

    // Inject procmgr endpoint (for frame allocation).
    inject_cap(
        caps.procmgr_ep,
        0, // sentinel: aux0=0, aux1=0
        child_cspace,
        &mut descs,
        &mut desc_count,
        &mut first_slot,
    );

    // Inject driver service endpoint (receive cap).
    if let Ok(child_slot) = syscall::cap_copy(driver_ep, child_cspace, !0u64)
    {
        if desc_count == 0
        {
            first_slot = child_slot;
        }
        if desc_count < MAX_DRIVER_DESCS
        {
            descs[desc_count] = CapDescriptor {
                slot: child_slot,
                cap_type: CapType::Frame,
                pad: [0; 3],
                aux0: SERVICE_ENDPOINT_SENTINEL,
                aux1: 0,
            };
            desc_count += 1;
        }
    }

    // Phase 3: Patch ProcessInfo.
    if syscall::mem_map(
        pi_frame,
        caps.self_aspace,
        CHILD_PI_VA,
        0,
        1,
        syscall::PROT_WRITE,
    )
    .is_err()
    {
        log("vfsd: cannot map fatfs ProcessInfo");
        return None;
    }

    // SAFETY: CHILD_PI_VA is mapped writable to the ProcessInfo page.
    #[allow(clippy::cast_ptr_alignment)]
    let pi = unsafe { &mut *(CHILD_PI_VA as *mut ProcessInfo) };

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
        // SAFETY: byte_offset is within the mapped page; alignment is correct.
        #[allow(clippy::cast_ptr_alignment)]
        unsafe {
            let ptr = (CHILD_PI_VA as *mut u8)
                .add(byte_offset)
                .cast::<CapDescriptor>();
            core::ptr::write(ptr, *desc);
        }
    }

    let _ = syscall::mem_unmap(caps.self_aspace, CHILD_PI_VA, 1);

    // Phase 4: START_PROCESS.
    // SAFETY: writing pid to IPC buffer.
    unsafe { core::ptr::write_volatile(ipc_buf, pid) };
    if let Ok((0, _)) = syscall::ipc_call(caps.procmgr_ep, LABEL_START_PROCESS, 1, &[])
    {
        log("vfsd: fatfs driver started");
    }
    else
    {
        log("vfsd: fatfs START_PROCESS failed");
        return None;
    }

    Some(driver_ep)
}

/// Derive-twice and inject a cap described by a Frame-type sentinel.
fn inject_cap(
    src_slot: u32,
    sentinel_aux0: u64,
    child_cspace: u32,
    descs: &mut [CapDescriptor; MAX_DRIVER_DESCS],
    desc_count: &mut usize,
    first_slot: &mut u32,
)
{
    let Ok(intermediary) = syscall::cap_derive(src_slot, !0u64)
    else
    {
        return;
    };
    let Ok(child_slot) = syscall::cap_copy(intermediary, child_cspace, !0u64)
    else
    {
        return;
    };
    if *desc_count == 0
    {
        *first_slot = child_slot;
    }
    if *desc_count < MAX_DRIVER_DESCS
    {
        descs[*desc_count] = CapDescriptor {
            slot: child_slot,
            cap_type: CapType::Frame,
            pad: [0; 3],
            aux0: sentinel_aux0,
            aux1: 0,
        };
        *desc_count += 1;
    }
}

// ── IPC helpers ─────────────────────────────────────────────────────────────

/// Read path bytes from the IPC buffer (packed into data words).
fn read_path_from_ipc(ipc_buf: *const u64, path_len: usize, buf: &mut [u8; 48])
{
    let word_count = path_len.div_ceil(8).min(6);
    for i in 0..word_count
    {
        // SAFETY: ipc_buf is valid, i < MSG_DATA_WORDS_MAX.
        let word = unsafe { core::ptr::read_volatile(ipc_buf.add(i)) };
        let base = i * 8;
        for j in 0..8
        {
            if base + j < path_len
            {
                buf[base + j] = ((word >> (j * 8)) & 0xFF) as u8;
            }
        }
    }
}

/// Write path bytes into the IPC buffer for forwarding to a driver.
fn write_path_to_ipc(ipc_buf: *mut u64, path: &[u8])
{
    let word_count = path.len().div_ceil(8).min(6);
    for i in 0..word_count
    {
        let mut word: u64 = 0;
        let base = i * 8;
        for j in 0..8
        {
            if base + j < path.len()
            {
                word |= u64::from(path[base + j]) << (j * 8);
            }
        }
        // SAFETY: ipc_buf is valid, i < MSG_DATA_WORDS_MAX.
        unsafe { core::ptr::write_volatile(ipc_buf.add(i), word) };
    }
}

// ── Entry point ─────────────────────────────────────────────────────────────

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

    log("vfsd: starting");

    // SAFETY: IPC buffer is page-aligned.
    #[allow(clippy::cast_ptr_alignment)]
    let ipc_buf = startup.ipc_buffer.cast::<u64>();

    if caps.service_ep == 0 || caps.registry_ep == 0
    {
        log("vfsd: missing required endpoints");
        idle_loop();
    }

    // Query devmgr for the block device endpoint.
    log("vfsd: querying devmgr for block device");
    let Ok((reply_label, _)) =
        syscall::ipc_call(caps.registry_ep, LABEL_QUERY_BLOCK_DEVICE, 0, &[])
    else
    {
        log("vfsd: QUERY_BLOCK_DEVICE ipc_call failed");
        idle_loop();
    };
    if reply_label != 0
    {
        log("vfsd: no block device available");
        idle_loop();
    }

    // SAFETY: ipc_buf is the registered IPC buffer.
    #[allow(clippy::cast_ptr_alignment)]
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc_buf) };
    if cap_count == 0
    {
        log("vfsd: QUERY_BLOCK_DEVICE returned no caps");
        idle_loop();
    }
    let blk_ep = reply_caps[0];
    log("vfsd: block device endpoint acquired");

    // Parse GPT partition table — stored for UUID lookups on MOUNT requests.
    let mut gpt_parts = [
        GptEntry::empty(),
        GptEntry::empty(),
        GptEntry::empty(),
        GptEntry::empty(),
        GptEntry::empty(),
        GptEntry::empty(),
        GptEntry::empty(),
        GptEntry::empty(),
    ];
    let _gpt_count = parse_gpt(blk_ep, ipc_buf, &mut gpt_parts);

    log("vfsd: entering service loop");
    service_loop(caps.service_ep, ipc_buf, &caps, blk_ep, &gpt_parts);
}

// ── GPT parsing ────────────────────────────────────────────────────────────

/// Block read helper: read a sector into `buf` via the block device endpoint.
fn read_block_sector(
    blk_ep: u32,
    sector: u64,
    buf: &mut [u8; SECTOR_SIZE],
    ipc_buf: *mut u64,
) -> bool
{
    // SAFETY: IPC buffer is valid.
    unsafe { core::ptr::write_volatile(ipc_buf, sector) };

    let Ok((reply_label, _)) = syscall::ipc_call(blk_ep, LABEL_READ_BLOCK, 1, &[])
    else
    {
        return false;
    };
    if reply_label != 0
    {
        return false;
    }

    // Copy sector data from IPC buffer BEFORE any log() calls.
    for i in 0..64
    {
        // SAFETY: IPC buffer is valid; i < 64 (512 bytes total).
        let word = unsafe { core::ptr::read_volatile(ipc_buf.add(i)) };
        let base = i * 8;
        let bytes = word.to_le_bytes();
        buf[base..base + 8].copy_from_slice(&bytes);
    }

    true
}

/// IPC label for block device reads.
const LABEL_READ_BLOCK: u64 = 1;

/// Parse the GPT and populate a partition table with UUID and LBA for
/// each non-empty partition. Returns the number of entries found.
fn parse_gpt(blk_ep: u32, ipc_buf: *mut u64, parts: &mut [GptEntry; MAX_GPT_PARTS]) -> usize
{
    let mut sector = [0u8; SECTOR_SIZE];

    // Read GPT header at LBA 1.
    if !read_block_sector(blk_ep, 1, &mut sector, ipc_buf)
    {
        log("vfsd: GPT: failed to read header");
        return 0;
    }

    // Validate signature "EFI PART".
    if &sector[0..8] != b"EFI PART"
    {
        log("vfsd: GPT: invalid signature");
        return 0;
    }

    let part_entry_lba = u64::from_le_bytes(sector[72..80].try_into().unwrap_or([0; 8]));
    let num_parts = u32::from_le_bytes(sector[80..84].try_into().unwrap_or([0; 4]));
    let entry_size = u32::from_le_bytes(sector[84..88].try_into().unwrap_or([0; 4]));

    if entry_size == 0 || entry_size > 512
    {
        log("vfsd: GPT: invalid entry size");
        return 0;
    }

    let entries_per_sector = SECTOR_SIZE as u32 / entry_size;
    let sectors_needed = num_parts.div_ceil(entries_per_sector);
    let mut found: usize = 0;
    let mut entries_checked: u32 = 0;

    for s in 0..sectors_needed
    {
        if found >= MAX_GPT_PARTS
        {
            break;
        }
        if !read_block_sector(blk_ep, part_entry_lba + u64::from(s), &mut sector, ipc_buf)
        {
            break;
        }

        for e in 0..entries_per_sector
        {
            if entries_checked >= num_parts || found >= MAX_GPT_PARTS
            {
                break;
            }
            let off = (e * entry_size) as usize;
            let first_lba =
                u64::from_le_bytes(sector[off + 32..off + 40].try_into().unwrap_or([0; 8]));

            // Skip empty entries.
            if first_lba == 0
            {
                entries_checked += 1;
                continue;
            }

            // Store unique partition UUID (bytes 16..32) and LBA.
            let mut uuid = [0u8; 16];
            uuid.copy_from_slice(&sector[off + 16..off + 32]);
            parts[found] = GptEntry {
                uuid,
                first_lba,
                active: true,
            };
            runtime::log::log_hex("vfsd: GPT: partition at LBA ", first_lba);
            found += 1;
            entries_checked += 1;
        }
    }

    runtime::log::log_hex("vfsd: GPT: partitions found: ", found as u64);
    found
}

/// Look up a partition UUID in the GPT table. Returns the LBA offset or 0.
fn lookup_partition_by_uuid(uuid: &[u8; 16], parts: &[GptEntry; MAX_GPT_PARTS]) -> u64
{
    for p in parts
    {
        if p.active && p.uuid == *uuid
        {
            return p.first_lba;
        }
    }
    0
}

/// Main VFS service loop — receive client requests and dispatch to drivers.
///
/// Handles both namespace operations (OPEN/READ/CLOSE/STAT/READDIR) and
/// MOUNT requests. MOUNT looks up a partition UUID in the GPT table, spawns
/// a fatfs driver with the appropriate LBA offset, and registers the mount.
#[allow(clippy::too_many_arguments)]
fn service_loop(
    service_ep: u32,
    ipc_buf: *mut u64,
    caps: &VfsdCaps,
    blk_ep: u32,
    gpt_parts: &[GptEntry; MAX_GPT_PARTS],
) -> !
{
    let mut mounts = [
        MountEntry::empty(),
        MountEntry::empty(),
        MountEntry::empty(),
        MountEntry::empty(),
    ];
    let mut fds = [
        FdEntry::empty(),
        FdEntry::empty(),
        FdEntry::empty(),
        FdEntry::empty(),
        FdEntry::empty(),
        FdEntry::empty(),
        FdEntry::empty(),
        FdEntry::empty(),
        FdEntry::empty(),
        FdEntry::empty(),
        FdEntry::empty(),
        FdEntry::empty(),
        FdEntry::empty(),
        FdEntry::empty(),
        FdEntry::empty(),
        FdEntry::empty(),
    ];

    loop
    {
        let Ok((label, _data_count)) = syscall::ipc_recv(service_ep)
        else
        {
            continue;
        };

        let opcode = label & 0xFFFF;

        match opcode
        {
            LABEL_OPEN => handle_open(label, ipc_buf, &mounts, &mut fds),
            LABEL_READ => handle_read(ipc_buf, &mounts, &fds),
            LABEL_CLOSE => handle_close(ipc_buf, &mut fds),
            LABEL_STAT => handle_stat(ipc_buf, &mounts, &fds),
            LABEL_READDIR => handle_readdir(ipc_buf, &mounts, &fds),
            LABEL_FS_MOUNT =>
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
    gpt_parts: &[GptEntry; MAX_GPT_PARTS],
    mounts: &mut [MountEntry; MAX_MOUNTS],
)
{
    // Read UUID from data[0..2] (16 bytes).
    // SAFETY: IPC buffer is valid and word-aligned.
    let w0 = unsafe { core::ptr::read_volatile(ipc_buf) };
    // SAFETY: see above.
    let w1 = unsafe { core::ptr::read_volatile(ipc_buf.add(1)) };
    let mut uuid = [0u8; 16];
    uuid[..8].copy_from_slice(&w0.to_le_bytes());
    uuid[8..].copy_from_slice(&w1.to_le_bytes());

    // Read path length and path bytes.
    // SAFETY: IPC buffer is valid.
    let path_len = unsafe { core::ptr::read_volatile(ipc_buf.add(2)) } as usize;
    if path_len == 0 || path_len > 64
    {
        log("vfsd: MOUNT: invalid path length");
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
    let partition_lba = lookup_partition_by_uuid(&uuid, gpt_parts);
    if partition_lba == 0
    {
        log("vfsd: MOUNT: partition UUID not found");
        let _ = syscall::ipc_reply(2, 0, &[]);
        return;
    }
    runtime::log::log_hex("vfsd: MOUNT: partition LBA=", partition_lba);

    // Spawn fatfs driver for this partition.
    if caps.fatfs_module_cap == 0
    {
        log("vfsd: MOUNT: no fatfs module cap");
        let _ = syscall::ipc_reply(3, 0, &[]);
        return;
    }

    let Some(driver_ep) = spawn_fatfs_driver(caps, blk_ep, ipc_buf)
    else
    {
        log("vfsd: MOUNT: failed to spawn fatfs");
        let _ = syscall::ipc_reply(4, 0, &[]);
        return;
    };

    // Send FS_MOUNT to the fatfs driver with the partition LBA offset.
    // SAFETY: IPC buffer is valid.
    unsafe { core::ptr::write_volatile(ipc_buf, partition_lba) };
    if let Ok((0, _)) = syscall::ipc_call(driver_ep, LABEL_FS_MOUNT, 1, &[])
    {
        log("vfsd: MOUNT: fatfs mounted successfully");
    }
    else
    {
        log("vfsd: MOUNT: fatfs FS_MOUNT failed");
        let _ = syscall::ipc_reply(5, 0, &[]);
        return;
    }

    // Register mount entry (longest-prefix matching in resolve_mount).
    let mut slot = None;
    for (i, m) in mounts.iter().enumerate()
    {
        if !m.active
        {
            slot = Some(i);
            break;
        }
    }

    let Some(idx) = slot
    else
    {
        log("vfsd: MOUNT: mount table full");
        let _ = syscall::ipc_reply(6, 0, &[]);
        return;
    };

    mounts[idx].path[..path_len].copy_from_slice(&path_buf[..path_len]);
    mounts[idx].path_len = path_len;
    mounts[idx].driver_ep = driver_ep;
    mounts[idx].active = true;

    log("vfsd: MOUNT: registered");
    let _ = syscall::ipc_reply(0, 0, &[]);
}

// ── Operation handlers ──────────────────────────────────────────────────────

fn handle_open(
    label: u64,
    ipc_buf: *mut u64,
    mounts: &[MountEntry; MAX_MOUNTS],
    fds: &mut [FdEntry; MAX_FDS],
)
{
    let path_len = ((label >> 16) & 0xFFFF) as usize;
    if path_len == 0 || path_len > 48
    {
        let _ = syscall::ipc_reply(1, 0, &[]); // NotFound
        return;
    }

    let mut path_buf = [0u8; 48];
    read_path_from_ipc(ipc_buf, path_len, &mut path_buf);
    let path = &path_buf[..path_len];

    let Some((mount_idx, driver_path)) = resolve_mount(path, mounts)
    else
    {
        let _ = syscall::ipc_reply(2, 0, &[]); // NoMount
        return;
    };

    let driver_ep = mounts[mount_idx].driver_ep;

    // Forward FS_OPEN to driver with the driver-relative path.
    let fwd_path_len = driver_path.len();
    write_path_to_ipc(ipc_buf, driver_path);

    let fwd_label = LABEL_OPEN | ((fwd_path_len as u64) << 16);
    let data_words = fwd_path_len.div_ceil(8).min(6);
    let Ok((drv_reply, _)) = syscall::ipc_call(driver_ep, fwd_label, data_words, &[])
    else
    {
        let _ = syscall::ipc_reply(5, 0, &[]); // IoError
        return;
    };

    if drv_reply != 0
    {
        let _ = syscall::ipc_reply(drv_reply, 0, &[]);
        return;
    }

    // Read driver fd from reply.
    // SAFETY: IPC buffer is valid.
    let driver_fd = unsafe { core::ptr::read_volatile(ipc_buf) };

    let Some(vfsd_fd) = alloc_fd(fds, mount_idx, driver_fd)
    else
    {
        // Fd table full — close the driver fd.
        // SAFETY: writing driver_fd to IPC buffer.
        unsafe { core::ptr::write_volatile(ipc_buf, driver_fd) };
        let _ = syscall::ipc_call(driver_ep, LABEL_CLOSE, 1, &[]);
        let _ = syscall::ipc_reply(3, 0, &[]); // TooManyOpen
        return;
    };

    // Reply with vfsd fd.
    // SAFETY: writing fd to IPC buffer.
    unsafe { core::ptr::write_volatile(ipc_buf, vfsd_fd) };
    let _ = syscall::ipc_reply(0, 1, &[]);
}

fn handle_read(ipc_buf: *mut u64, mounts: &[MountEntry; MAX_MOUNTS], fds: &[FdEntry; MAX_FDS])
{
    // Read request: data[0]=fd, data[1]=offset, data[2]=max_len.
    // SAFETY: IPC buffer is valid and word-aligned; indices 0–2 are within
    // MSG_DATA_WORDS_MAX.
    let fd_idx = unsafe { core::ptr::read_volatile(ipc_buf) };
    // SAFETY: see above.
    let offset = unsafe { core::ptr::read_volatile(ipc_buf.add(1)) };
    // SAFETY: see above.
    let max_len = unsafe { core::ptr::read_volatile(ipc_buf.add(2)) };

    let idx = fd_idx as usize;
    if idx >= MAX_FDS || !fds[idx].in_use
    {
        let _ = syscall::ipc_reply(4, 0, &[]); // InvalidFd
        return;
    }

    let mount_idx = fds[idx].mount_idx;
    let driver_fd = fds[idx].driver_fd;
    let driver_ep = mounts[mount_idx].driver_ep;

    // Forward FS_READ to driver.
    // SAFETY: writing request data to IPC buffer.
    unsafe {
        core::ptr::write_volatile(ipc_buf, driver_fd);
        core::ptr::write_volatile(ipc_buf.add(1), offset);
        core::ptr::write_volatile(ipc_buf.add(2), max_len);
    }

    let Ok((drv_reply, _)) = syscall::ipc_call(driver_ep, LABEL_READ, 3, &[])
    else
    {
        let _ = syscall::ipc_reply(5, 0, &[]); // IoError
        return;
    };

    if drv_reply != 0
    {
        let _ = syscall::ipc_reply(drv_reply, 0, &[]);
        return;
    }

    // Relay the driver's reply. Derive word count from bytes_read (data[0]).
    // SAFETY: IPC buffer is valid.
    let bytes_read = unsafe { core::ptr::read_volatile(ipc_buf) } as usize;
    let reply_words = 1 + bytes_read.div_ceil(8);
    let _ = syscall::ipc_reply(0, reply_words, &[]);
}

fn handle_close(ipc_buf: *mut u64, fds: &mut [FdEntry; MAX_FDS])
{
    // SAFETY: IPC buffer is valid.
    let fd_idx = unsafe { core::ptr::read_volatile(ipc_buf) };

    let idx = fd_idx as usize;
    if idx >= MAX_FDS || !fds[idx].in_use
    {
        let _ = syscall::ipc_reply(4, 0, &[]); // InvalidFd
        return;
    }

    // We don't forward CLOSE to the driver yet — driver fd entries are
    // long-lived. Just free the vfsd fd.
    // TODO: forward FS_CLOSE to driver for proper resource cleanup.
    free_fd(fds, fd_idx);
    let _ = syscall::ipc_reply(0, 0, &[]);
}

fn handle_stat(ipc_buf: *mut u64, mounts: &[MountEntry; MAX_MOUNTS], fds: &[FdEntry; MAX_FDS])
{
    // SAFETY: IPC buffer is valid.
    let fd_idx = unsafe { core::ptr::read_volatile(ipc_buf) };

    let idx = fd_idx as usize;
    if idx >= MAX_FDS || !fds[idx].in_use
    {
        let _ = syscall::ipc_reply(4, 0, &[]); // InvalidFd
        return;
    }

    let mount_idx = fds[idx].mount_idx;
    let driver_fd = fds[idx].driver_fd;
    let driver_ep = mounts[mount_idx].driver_ep;

    // Forward FS_STAT to driver.
    // SAFETY: writing driver_fd to IPC buffer.
    unsafe { core::ptr::write_volatile(ipc_buf, driver_fd) };

    let Ok((drv_reply, drv_data_count)) = syscall::ipc_call(driver_ep, LABEL_STAT, 1, &[])
    else
    {
        let _ = syscall::ipc_reply(5, 0, &[]); // IoError
        return;
    };

    let _ = syscall::ipc_reply(drv_reply, drv_data_count, &[]);
}

fn handle_readdir(ipc_buf: *mut u64, mounts: &[MountEntry; MAX_MOUNTS], fds: &[FdEntry; MAX_FDS])
{
    // SAFETY: IPC buffer is valid and word-aligned.
    let fd_idx = unsafe { core::ptr::read_volatile(ipc_buf) };
    // SAFETY: see above.
    let entry_idx = unsafe { core::ptr::read_volatile(ipc_buf.add(1)) };

    let idx = fd_idx as usize;
    if idx >= MAX_FDS || !fds[idx].in_use
    {
        let _ = syscall::ipc_reply(4, 0, &[]); // InvalidFd
        return;
    }

    let mount_idx = fds[idx].mount_idx;
    let driver_fd = fds[idx].driver_fd;
    let driver_ep = mounts[mount_idx].driver_ep;

    // Forward FS_READDIR to driver.
    // SAFETY: writing request data to IPC buffer.
    unsafe {
        core::ptr::write_volatile(ipc_buf, driver_fd);
        core::ptr::write_volatile(ipc_buf.add(1), entry_idx);
    }

    let Ok((drv_reply, drv_data_count)) = syscall::ipc_call(driver_ep, LABEL_READDIR, 2, &[])
    else
    {
        let _ = syscall::ipc_reply(5, 0, &[]); // IoError
        return;
    };

    let _ = syscall::ipc_reply(drv_reply, drv_data_count, &[]);
}

fn idle_loop() -> !
{
    loop
    {
        let _ = syscall::thread_yield();
    }
}
