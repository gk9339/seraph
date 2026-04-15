// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// fs/fat/src/main.rs

//! Seraph FAT filesystem driver.
//!
//! Implements read-only FAT16/FAT32 filesystem support. Receives IPC requests
//! from vfsd conforming to `fs/docs/fs-driver-protocol.md`. All disk I/O is
//! performed via the block device IPC endpoint received at creation time.

#![no_std]
#![no_main]
#![allow(clippy::cast_possible_truncation)]

extern crate runtime;

use process_abi::{CapType, StartupInfo};

// ── Constants ────────────────────────────────────────────────────────────────

const SECTOR_SIZE: usize = 512;

// IPC labels — fs-driver-protocol.
const LABEL_FS_OPEN: u64 = 1;
const LABEL_FS_READ: u64 = 2;
const LABEL_FS_CLOSE: u64 = 3;
const LABEL_FS_STAT: u64 = 4;
const LABEL_FS_READDIR: u64 = 5;
const LABEL_FS_MOUNT: u64 = 10;

/// End-of-directory reply label.
const LABEL_END_OF_DIR: u64 = 6;

// IPC label for block device.
const LABEL_READ_BLOCK: u64 = 1;

// Sentinel values.
const LOG_ENDPOINT_SENTINEL: u64 = 0xFFFF_FFFF_FFFF_FFFF;
const SERVICE_ENDPOINT_SENTINEL: u64 = 0xFFFF_FFFF_FFFF_FFFE;
const BLOCK_ENDPOINT_SENTINEL: u64 = 0xFFFF_FFFF_FFFF_FFFC;

/// Maximum open file descriptors.
const MAX_FDS: usize = 8;

// ── FAT structures ──────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum FatType
{
    Fat16,
    Fat32,
}

struct FatState
{
    fat_type: FatType,
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors: u16,
    num_fats: u8,
    root_entry_count: u16, // FAT16 only
    fat_size: u32,         // sectors per FAT
    root_cluster: u32,     // FAT32 only
    data_start_sector: u32,
    /// LBA offset of the partition on the raw disk.
    /// Added to all sector numbers before issuing block reads.
    partition_offset: u64,
    /// Cached FAT sector number (avoids re-reads for sequential access).
    cached_fat_sector: u32,
    /// Cached FAT sector data.
    cached_fat_data: [u8; SECTOR_SIZE],
}

impl FatState
{
    fn new() -> Self
    {
        Self {
            fat_type: FatType::Fat32,
            bytes_per_sector: 512,
            sectors_per_cluster: 1,
            reserved_sectors: 0,
            num_fats: 2,
            root_entry_count: 0,
            fat_size: 0,
            root_cluster: 2,
            data_start_sector: 0,
            partition_offset: 0,
            cached_fat_sector: u32::MAX,
            cached_fat_data: [0; SECTOR_SIZE],
        }
    }

    /// First sector of a given cluster.
    fn cluster_to_sector(&self, cluster: u32) -> u32
    {
        self.data_start_sector + (cluster - 2) * u32::from(self.sectors_per_cluster)
    }

    /// Bytes per cluster.
    fn cluster_size(&self) -> u32
    {
        u32::from(self.sectors_per_cluster) * u32::from(self.bytes_per_sector)
    }
}

struct FatFd
{
    start_cluster: u32,
    file_size: u32,
    is_dir: bool,
    in_use: bool,
}

impl FatFd
{
    const fn empty() -> Self
    {
        Self {
            start_cluster: 0,
            file_size: 0,
            is_dir: false,
            in_use: false,
        }
    }
}

struct FatCaps
{
    block_dev: u32,
    log_sink: u32,
    service: u32,
}

// ── Cap classification ──────────────────────────────────────────────────────

fn classify_caps(startup: &StartupInfo) -> FatCaps
{
    let mut caps = FatCaps {
        block_dev: 0,
        log_sink: 0,
        service: 0,
    };

    for d in startup.initial_caps
    {
        if d.cap_type == CapType::Frame
        {
            if d.aux0 == LOG_ENDPOINT_SENTINEL
            {
                caps.log_sink = d.slot;
            }
            else if d.aux0 == SERVICE_ENDPOINT_SENTINEL
            {
                caps.service = d.slot;
            }
            else if d.aux0 == BLOCK_ENDPOINT_SENTINEL
            {
                caps.block_dev = d.slot;
            }
        }
    }

    caps
}

// ── Block I/O ───────────────────────────────────────────────────────────────

/// Read a single 512-byte sector from the block device into `buf`.
///
/// `sector` is partition-relative. The partition offset is added before
/// issuing the block read to translate to an absolute disk LBA.
fn read_sector(
    block_dev: u32,
    sector: u64,
    buf: &mut [u8; SECTOR_SIZE],
    ipc_buf: *mut u64,
    partition_offset: u64,
) -> bool
{
    // SAFETY: IPC buffer is valid.
    unsafe { core::ptr::write_volatile(ipc_buf, sector + partition_offset) };

    let Ok((reply_label, _data_count)) = syscall::ipc_call(block_dev, LABEL_READ_BLOCK, 1, &[])
    else
    {
        return false;
    };
    if reply_label != 0
    {
        return false;
    }

    // Copy sector data from IPC buffer BEFORE any log() calls — log() uses
    // the same IPC buffer and would overwrite the reply data.
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

// ── BPB parsing ─────────────────────────────────────────────────────────────

/// Parse the BIOS Parameter Block from sector 0.
fn parse_bpb(sector_data: &[u8; SECTOR_SIZE], state: &mut FatState) -> bool
{
    // Validate boot signature.
    if sector_data[510] != 0x55 || sector_data[511] != 0xAA
    {
        runtime::log!("fatfs: invalid boot signature");
        return false;
    }

    state.bytes_per_sector = u16::from_le_bytes([sector_data[11], sector_data[12]]);
    state.sectors_per_cluster = sector_data[13];
    state.reserved_sectors = u16::from_le_bytes([sector_data[14], sector_data[15]]);
    state.num_fats = sector_data[16];
    state.root_entry_count = u16::from_le_bytes([sector_data[17], sector_data[18]]);

    // Validate fields used as divisors to prevent division by zero.
    if state.bytes_per_sector == 0 || state.sectors_per_cluster == 0
    {
        runtime::log!("fatfs: invalid BPB: bytes_per_sector or sectors_per_cluster is zero");
        return false;
    }

    let total_sectors_16 = u16::from_le_bytes([sector_data[19], sector_data[20]]);
    let fat_size_16 = u16::from_le_bytes([sector_data[22], sector_data[23]]);
    let total_sectors_32 = u32::from_le_bytes([
        sector_data[32],
        sector_data[33],
        sector_data[34],
        sector_data[35],
    ]);

    // FAT32 extended BPB.
    let fat_size_32 = u32::from_le_bytes([
        sector_data[36],
        sector_data[37],
        sector_data[38],
        sector_data[39],
    ]);
    state.root_cluster = u32::from_le_bytes([
        sector_data[44],
        sector_data[45],
        sector_data[46],
        sector_data[47],
    ]);

    state.fat_size = if fat_size_16 != 0
    {
        u32::from(fat_size_16)
    }
    else
    {
        fat_size_32
    };

    let total_sectors = if total_sectors_16 != 0
    {
        u32::from(total_sectors_16)
    }
    else
    {
        total_sectors_32
    };

    // Root directory sectors (FAT16 only).
    let root_dir_sectors =
        (u32::from(state.root_entry_count) * 32).div_ceil(u32::from(state.bytes_per_sector));

    state.data_start_sector = u32::from(state.reserved_sectors)
        + u32::from(state.num_fats) * state.fat_size
        + root_dir_sectors;

    let data_sectors = total_sectors.saturating_sub(state.data_start_sector);
    let total_clusters = data_sectors / u32::from(state.sectors_per_cluster);

    // FAT type determination per Microsoft specification.
    if total_clusters < 65525
    {
        state.fat_type = FatType::Fat16;
        runtime::log!("fatfs: detected FAT16");
    }
    else
    {
        state.fat_type = FatType::Fat32;
        runtime::log!("fatfs: detected FAT32");
    }

    runtime::log!(
        "fatfs: sectors_per_cluster={:#018x}",
        u64::from(state.sectors_per_cluster)
    );
    runtime::log!("fatfs: total_clusters={:#018x}", u64::from(total_clusters));
    runtime::log!(
        "fatfs: data_start_sector={:#018x}",
        u64::from(state.data_start_sector)
    );

    true
}

// ── FAT lookup ──────────────────────────────────────────────────────────────

/// Look up the next cluster in the FAT chain.
///
/// Returns `Some(next)` for a valid next cluster, `None` for end-of-chain
/// or bad cluster.
fn next_cluster(
    state: &mut FatState,
    cluster: u32,
    block_dev: u32,
    ipc_buf: *mut u64,
) -> Option<u32>
{
    let (fat_offset, fat_sector, entry_offset) = match state.fat_type
    {
        FatType::Fat16 =>
        {
            let offset = cluster * 2;
            let sector =
                u32::from(state.reserved_sectors) + offset / u32::from(state.bytes_per_sector);
            let ent_off = (offset % u32::from(state.bytes_per_sector)) as usize;
            (offset, sector, ent_off)
        }
        FatType::Fat32 =>
        {
            let offset = cluster * 4;
            let sector =
                u32::from(state.reserved_sectors) + offset / u32::from(state.bytes_per_sector);
            let ent_off = (offset % u32::from(state.bytes_per_sector)) as usize;
            (offset, sector, ent_off)
        }
    };
    let _ = fat_offset; // Used only to compute sector and entry_offset.

    // Read the FAT sector (with caching).
    if state.cached_fat_sector != fat_sector
    {
        if !read_sector(
            block_dev,
            u64::from(fat_sector),
            &mut state.cached_fat_data,
            ipc_buf,
            state.partition_offset,
        )
        {
            return None;
        }
        state.cached_fat_sector = fat_sector;
    }

    let val = match state.fat_type
    {
        FatType::Fat16 =>
        {
            let raw = u16::from_le_bytes([
                state.cached_fat_data[entry_offset],
                state.cached_fat_data[entry_offset + 1],
            ]);
            if raw >= 0xFFF8
            {
                return None; // End of chain.
            }
            if raw == 0xFFF7
            {
                return None; // Bad cluster.
            }
            u32::from(raw)
        }
        FatType::Fat32 =>
        {
            let raw = u32::from_le_bytes([
                state.cached_fat_data[entry_offset],
                state.cached_fat_data[entry_offset + 1],
                state.cached_fat_data[entry_offset + 2],
                state.cached_fat_data[entry_offset + 3],
            ]) & 0x0FFF_FFFF;
            if raw >= 0x0FFF_FFF8
            {
                return None; // End of chain.
            }
            if raw == 0x0FFF_FFF7
            {
                return None; // Bad cluster.
            }
            raw
        }
    };

    Some(val)
}

// ── Directory parsing ───────────────────────────────────────────────────────

/// A parsed directory entry (8.3 or LFN).
#[derive(Clone, Copy)]
struct DirEntry
{
    name: [u8; 11], // 8.3 name (space-padded)
    attr: u8,
    cluster: u32,
    size: u32,
}

/// LFN accumulator for assembling long file names across directory entries.
///
/// LFN entries appear in reverse order before their associated 8.3 entry.
/// Each LFN entry carries up to 13 UCS-2 characters. We store them as
/// ASCII bytes (upper byte discarded — sufficient for standard filenames).
struct LfnAccum
{
    buf: [u8; 255],
    len: usize,
    active: bool,
}

impl LfnAccum
{
    const fn new() -> Self
    {
        Self {
            buf: [0; 255],
            len: 0,
            active: false,
        }
    }

    fn reset(&mut self)
    {
        self.len = 0;
        self.active = false;
    }

    /// Process an LFN directory entry. Extracts characters and places them
    /// at the correct position based on the sequence number.
    fn add_lfn_entry(&mut self, raw: &[u8])
    {
        let seq = raw[0] & 0x3F;
        if seq == 0
        {
            self.reset();
            return;
        }
        self.active = true;

        // Each LFN entry carries 13 UCS-2 characters.
        let base_idx = (seq as usize - 1) * 13;

        // Character positions within the 32-byte entry:
        // bytes 1-10 (5 chars), 14-25 (6 chars), 28-31 (2 chars).
        let offsets: [usize; 13] = [1, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30];
        for (i, &off) in offsets.iter().enumerate()
        {
            let pos = base_idx + i;
            if pos >= 255
            {
                break;
            }
            let ch = u16::from_le_bytes([raw[off], raw[off + 1]]);
            if ch == 0 || ch == 0xFFFF
            {
                if pos > self.len
                {
                    self.len = pos;
                }
                return;
            }
            // Store as ASCII (discard upper byte — handles Latin-1 filenames).
            self.buf[pos] = (ch & 0xFF) as u8;
            if pos + 1 > self.len
            {
                self.len = pos + 1;
            }
        }
    }

    /// Check if the accumulated LFN matches a name (case-insensitive).
    fn matches(&self, name: &[u8]) -> bool
    {
        if !self.active || self.len != name.len()
        {
            return false;
        }
        for (i, &b) in name.iter().enumerate().take(self.len)
        {
            if to_upper(self.buf[i]) != to_upper(b)
            {
                return false;
            }
        }
        true
    }
}

/// Parse a single 32-byte FAT directory entry.
///
/// Returns `None` for end-of-directory (0x00), deleted (0xE5), and LFN
/// entries (attr=0x0F). LFN entries should be processed via `LfnAccum`
/// before calling this.
fn parse_dir_entry(raw: &[u8]) -> Option<DirEntry>
{
    if raw[0] == 0x00
    {
        return None; // End of directory.
    }
    if raw[0] == 0xE5
    {
        return None; // Deleted entry.
    }
    // Skip LFN entries.
    if raw[11] == 0x0F
    {
        return None;
    }

    let mut name = [0u8; 11];
    name.copy_from_slice(&raw[..11]);

    let attr = raw[11];
    let cluster_hi = u16::from_le_bytes([raw[20], raw[21]]);
    let cluster_lo = u16::from_le_bytes([raw[26], raw[27]]);
    let cluster = (u32::from(cluster_hi) << 16) | u32::from(cluster_lo);
    let size = u32::from_le_bytes([raw[28], raw[29], raw[30], raw[31]]);

    Some(DirEntry {
        name,
        attr,
        cluster,
        size,
    })
}

/// Check if an 8.3 directory entry name matches a path component.
///
/// The path component is converted to uppercase and compared against the
/// space-padded 8.3 name in the directory entry.
fn name_matches(entry_name: &[u8; 11], component: &[u8]) -> bool
{
    // Build an 8.3 name from the component.
    let mut padded = [b' '; 11];

    // Find dot position.
    let dot_pos = component.iter().position(|&b| b == b'.');

    let (base, ext) = if let Some(dp) = dot_pos
    {
        (&component[..dp], &component[dp + 1..])
    }
    else
    {
        (component, &[] as &[u8])
    };

    // Copy base name (up to 8 chars).
    for (i, &b) in base.iter().take(8).enumerate()
    {
        padded[i] = to_upper(b);
    }

    // Copy extension (up to 3 chars).
    for (i, &b) in ext.iter().take(3).enumerate()
    {
        padded[8 + i] = to_upper(b);
    }

    *entry_name == padded
}

fn to_upper(b: u8) -> u8
{
    if b.is_ascii_lowercase()
    {
        b - 32
    }
    else
    {
        b
    }
}

// ── Path resolution ─────────────────────────────────────────────────────────

/// Resolve a `/`-separated path from the root directory.
///
/// Returns the directory entry for the final component on success.
fn resolve_path(
    path: &[u8],
    state: &mut FatState,
    block_dev: u32,
    ipc_buf: *mut u64,
) -> Option<DirEntry>
{
    // Start from root.
    let mut current_cluster = match state.fat_type
    {
        FatType::Fat32 => state.root_cluster,
        FatType::Fat16 => 0, // Sentinel: FAT16 root is in a fixed area.
    };

    // Split path on '/'. Skip leading and trailing slashes.
    let path = strip_slashes(path);
    if path.is_empty()
    {
        // Root directory itself.
        return Some(DirEntry {
            name: *b"/          ",
            attr: 0x10,
            cluster: current_cluster,
            size: 0,
        });
    }

    let mut remaining = path;

    loop
    {
        // Extract next path component.
        let (component, rest) = split_component(remaining);
        if component.is_empty()
        {
            break;
        }

        // Search the current directory for the component.
        let entry = find_in_directory(current_cluster, component, state, block_dev, ipc_buf)?;

        if rest.is_empty()
        {
            // Final component — return it.
            return Some(entry);
        }

        // Intermediate component must be a directory.
        if entry.attr & 0x10 == 0
        {
            return None; // Not a directory.
        }

        current_cluster = entry.cluster;
        remaining = rest;
    }

    None
}

fn strip_slashes(path: &[u8]) -> &[u8]
{
    let mut start = 0;
    while start < path.len() && path[start] == b'/'
    {
        start += 1;
    }
    let mut end = path.len();
    while end > start && path[end - 1] == b'/'
    {
        end -= 1;
    }
    &path[start..end]
}

fn split_component(path: &[u8]) -> (&[u8], &[u8])
{
    if let Some(pos) = path.iter().position(|&b| b == b'/')
    {
        let rest = &path[pos + 1..];
        (&path[..pos], strip_slashes(rest))
    }
    else
    {
        (path, &[])
    }
}

/// Search a directory's cluster chain for an entry matching `name`.
fn find_in_directory(
    dir_cluster: u32,
    name: &[u8],
    state: &mut FatState,
    block_dev: u32,
    ipc_buf: *mut u64,
) -> Option<DirEntry>
{
    let mut sector_buf = [0u8; SECTOR_SIZE];
    let mut lfn = LfnAccum::new();

    if dir_cluster == 0
    {
        // FAT16 root directory: fixed location, not in cluster chain.
        return find_in_fat16_root(name, state, block_dev, ipc_buf);
    }

    let mut cluster = dir_cluster;
    loop
    {
        let base_sector = state.cluster_to_sector(cluster);
        for s in 0..u32::from(state.sectors_per_cluster)
        {
            if !read_sector(
                block_dev,
                u64::from(base_sector + s),
                &mut sector_buf,
                ipc_buf,
                state.partition_offset,
            )
            {
                return None;
            }
            if let Some(entry) = scan_sector_for_name(&sector_buf, name, &mut lfn)
            {
                return Some(entry);
            }
        }

        cluster = next_cluster(state, cluster, block_dev, ipc_buf)?;
    }
}

/// Search the FAT16 fixed root directory area.
fn find_in_fat16_root(
    name: &[u8],
    state: &mut FatState,
    block_dev: u32,
    ipc_buf: *mut u64,
) -> Option<DirEntry>
{
    let root_start = u32::from(state.reserved_sectors) + u32::from(state.num_fats) * state.fat_size;
    let root_sectors = (u32::from(state.root_entry_count) * 32).div_ceil(512);
    let mut sector_buf = [0u8; SECTOR_SIZE];
    let mut lfn = LfnAccum::new();

    for s in 0..root_sectors
    {
        if !read_sector(
            block_dev,
            u64::from(root_start + s),
            &mut sector_buf,
            ipc_buf,
            state.partition_offset,
        )
        {
            return None;
        }
        if let Some(entry) = scan_sector_for_name(&sector_buf, name, &mut lfn)
        {
            return Some(entry);
        }
    }

    None
}

/// Scan a sector's 32-byte directory entries for a name match.
///
/// Supports both 8.3 and LFN matching. The `lfn` accumulator carries
/// LFN state across sector boundaries.
fn scan_sector_for_name(
    sector: &[u8; SECTOR_SIZE],
    name: &[u8],
    lfn: &mut LfnAccum,
) -> Option<DirEntry>
{
    let entries_per_sector = SECTOR_SIZE / 32;
    for i in 0..entries_per_sector
    {
        let offset = i * 32;
        let raw = &sector[offset..offset + 32];
        if raw[0] == 0x00
        {
            return None; // End of directory.
        }
        if raw[0] == 0xE5
        {
            lfn.reset();
            continue; // Deleted entry.
        }
        if raw[11] == 0x0F
        {
            // LFN entry — accumulate.
            lfn.add_lfn_entry(raw);
            continue;
        }
        // Regular 8.3 entry. Check LFN match first, then 8.3.
        if let Some(entry) = parse_dir_entry(raw)
        {
            if lfn.matches(name) || name_matches(&entry.name, name)
            {
                return Some(entry);
            }
        }
        lfn.reset();
    }
    None
}

/// Read a directory entry at a given index.
fn read_dir_entry_at_index(
    dir_cluster: u32,
    index: u64,
    state: &mut FatState,
    block_dev: u32,
    ipc_buf: *mut u64,
) -> Option<DirEntry>
{
    let mut sector_buf = [0u8; SECTOR_SIZE];
    let entries_per_sector = SECTOR_SIZE / 32;
    let mut current_idx: u64 = 0;

    if dir_cluster == 0
    {
        // FAT16 fixed root.
        let root_start =
            u32::from(state.reserved_sectors) + u32::from(state.num_fats) * state.fat_size;
        let root_sectors = (u32::from(state.root_entry_count) * 32).div_ceil(512);

        for s in 0..root_sectors
        {
            if !read_sector(
                block_dev,
                u64::from(root_start + s),
                &mut sector_buf,
                ipc_buf,
                state.partition_offset,
            )
            {
                return None;
            }
            for e in 0..entries_per_sector
            {
                let offset = e * 32;
                let raw = &sector_buf[offset..offset + 32];
                if raw[0] == 0x00
                {
                    return None;
                }
                if let Some(entry) = parse_dir_entry(raw)
                {
                    if current_idx == index
                    {
                        return Some(entry);
                    }
                    current_idx += 1;
                }
            }
        }
        return None;
    }

    let mut cluster = dir_cluster;
    loop
    {
        let base_sector = state.cluster_to_sector(cluster);
        for s in 0..u32::from(state.sectors_per_cluster)
        {
            if !read_sector(
                block_dev,
                u64::from(base_sector + s),
                &mut sector_buf,
                ipc_buf,
                state.partition_offset,
            )
            {
                return None;
            }
            for e in 0..entries_per_sector
            {
                let offset = e * 32;
                let raw = &sector_buf[offset..offset + 32];
                if raw[0] == 0x00
                {
                    return None;
                }
                if let Some(entry) = parse_dir_entry(raw)
                {
                    if current_idx == index
                    {
                        return Some(entry);
                    }
                    current_idx += 1;
                }
            }
        }

        if let Some(next) = next_cluster(state, cluster, block_dev, ipc_buf)
        {
            cluster = next;
        }
        else
        {
            return None;
        }
    }
}

// ── File read ───────────────────────────────────────────────────────────────

/// Read up to 512 bytes from a file at a given byte offset.
///
/// Walks the cluster chain to find the correct cluster, reads the
/// relevant sector, and extracts the requested bytes.
///
/// Returns the number of bytes read (written to `out`).
#[allow(clippy::too_many_arguments)]
fn read_file_data(
    start_cluster: u32,
    file_size: u32,
    offset: u64,
    max_len: u64,
    state: &mut FatState,
    block_dev: u32,
    ipc_buf: *mut u64,
    out: &mut [u8; SECTOR_SIZE],
) -> usize
{
    if offset >= u64::from(file_size)
    {
        return 0; // Past EOF.
    }

    let remaining = u64::from(file_size) - offset;
    let to_read = max_len.min(remaining).min(SECTOR_SIZE as u64) as usize;

    let cluster_size = state.cluster_size();
    let cluster_idx = offset / u64::from(cluster_size);
    let offset_in_cluster = (offset % u64::from(cluster_size)) as u32;
    let sector_in_cluster = offset_in_cluster / u32::from(state.bytes_per_sector);
    let offset_in_sector = (offset_in_cluster % u32::from(state.bytes_per_sector)) as usize;

    // Walk the cluster chain to the target cluster.
    let mut cluster = start_cluster;
    for _ in 0..cluster_idx
    {
        cluster = match next_cluster(state, cluster, block_dev, ipc_buf)
        {
            Some(c) => c,
            None => return 0, // Premature end of chain.
        };
    }

    let sector = state.cluster_to_sector(cluster) + sector_in_cluster;
    let mut sector_buf = [0u8; SECTOR_SIZE];
    if !read_sector(
        block_dev,
        u64::from(sector),
        &mut sector_buf,
        ipc_buf,
        state.partition_offset,
    )
    {
        return 0;
    }

    // Extract bytes from the sector.
    let available = SECTOR_SIZE - offset_in_sector;
    let copy_len = to_read.min(available);
    out[..copy_len].copy_from_slice(&sector_buf[offset_in_sector..offset_in_sector + copy_len]);

    copy_len
}

// ── Entry point ─────────────────────────────────────────────────────────────

#[no_mangle]
extern "Rust" fn main(startup: &StartupInfo) -> !
{
    let _ = syscall::ipc_buffer_set(startup.ipc_buffer as u64);

    let caps = classify_caps(startup);

    if caps.log_sink != 0
    {
        // SAFETY: single-threaded; called once before any log calls.
        unsafe { runtime::log::log_init(caps.log_sink, startup.ipc_buffer) };
    }

    runtime::log!("fatfs: starting");

    // SAFETY: IPC buffer is page-aligned.
    #[allow(clippy::cast_ptr_alignment)]
    let ipc_buf = startup.ipc_buffer.cast::<u64>();

    if caps.block_dev == 0 || caps.service == 0
    {
        runtime::log!("fatfs: missing required caps");
        idle_loop();
    }

    let mut state = FatState::new();
    let mut fds = [
        FatFd::empty(),
        FatFd::empty(),
        FatFd::empty(),
        FatFd::empty(),
        FatFd::empty(),
        FatFd::empty(),
        FatFd::empty(),
        FatFd::empty(),
    ];

    service_loop(&caps, &mut state, &mut fds, ipc_buf);
}

// ── Service loop ────────────────────────────────────────────────────────────

fn service_loop(
    caps: &FatCaps,
    state: &mut FatState,
    fds: &mut [FatFd; MAX_FDS],
    ipc_buf: *mut u64,
) -> !
{
    let mut mounted = false;

    loop
    {
        let Ok((label, _data_count)) = syscall::ipc_recv(caps.service)
        else
        {
            continue;
        };

        let opcode = label & 0xFFFF;

        match opcode
        {
            LABEL_FS_MOUNT =>
            {
                if mounted
                {
                    let _ = syscall::ipc_reply(0, 0, &[]);
                    continue;
                }
                handle_mount(caps.block_dev, state, ipc_buf);
                mounted = true;
            }
            LABEL_FS_OPEN if mounted => handle_open(label, state, fds, caps.block_dev, ipc_buf),
            LABEL_FS_READ if mounted => handle_read(state, fds, caps.block_dev, ipc_buf),
            LABEL_FS_CLOSE if mounted => handle_close(fds, ipc_buf),
            LABEL_FS_STAT if mounted => handle_stat(fds, ipc_buf),
            LABEL_FS_READDIR if mounted => handle_readdir(state, fds, caps.block_dev, ipc_buf),
            _ =>
            {
                let _ = syscall::ipc_reply(0xFF, 0, &[]);
            }
        }
    }
}

// ── Operation handlers ──────────────────────────────────────────────────────

fn handle_mount(block_dev: u32, state: &mut FatState, ipc_buf: *mut u64)
{
    // Read partition LBA offset from FS_MOUNT data[0].
    // SAFETY: IPC buffer is valid.
    let partition_offset = unsafe { core::ptr::read_volatile(ipc_buf) };
    state.partition_offset = partition_offset;

    let mut sector_buf = [0u8; SECTOR_SIZE];
    // Read sector 0 of the partition (BPB), not sector 0 of the disk.
    if !read_sector(block_dev, 0, &mut sector_buf, ipc_buf, partition_offset)
    {
        runtime::log!("fatfs: failed to read partition sector 0");
        let _ = syscall::ipc_reply(2, 0, &[]); // IoError
        return;
    }

    if !parse_bpb(&sector_buf, state)
    {
        let _ = syscall::ipc_reply(1, 0, &[]); // InvalidFilesystem
        return;
    }

    runtime::log!("fatfs: filesystem mounted");
    let _ = syscall::ipc_reply(0, 0, &[]);
}

fn handle_open(
    label: u64,
    state: &mut FatState,
    fds: &mut [FatFd; MAX_FDS],
    block_dev: u32,
    ipc_buf: *mut u64,
)
{
    let path_len = ((label >> 16) & 0xFFFF) as usize;
    if path_len == 0 || path_len > 48
    {
        let _ = syscall::ipc_reply(1, 0, &[]); // NotFound
        return;
    }

    let mut path_buf = [0u8; 48];
    read_path_from_ipc(ipc_buf.cast_const(), path_len, &mut path_buf);
    let path = &path_buf[..path_len];

    let Some(entry) = resolve_path(path, state, block_dev, ipc_buf)
    else
    {
        let _ = syscall::ipc_reply(1, 0, &[]); // NotFound
        return;
    };

    // Allocate fd.
    let mut fd_idx = None;
    for (i, fd) in fds.iter_mut().enumerate()
    {
        if !fd.in_use
        {
            fd.in_use = true;
            fd.start_cluster = entry.cluster;
            fd.file_size = entry.size;
            fd.is_dir = entry.attr & 0x10 != 0;
            fd_idx = Some(i);
            break;
        }
    }

    let Some(idx) = fd_idx
    else
    {
        let _ = syscall::ipc_reply(3, 0, &[]); // TooManyOpen
        return;
    };

    // SAFETY: writing fd to IPC buffer.
    unsafe { core::ptr::write_volatile(ipc_buf, idx as u64) };
    let _ = syscall::ipc_reply(0, 1, &[]);
}

fn handle_read(state: &mut FatState, fds: &[FatFd; MAX_FDS], block_dev: u32, ipc_buf: *mut u64)
{
    // SAFETY: IPC buffer is valid and word-aligned.
    let fd_idx = unsafe { core::ptr::read_volatile(ipc_buf) } as usize;
    // SAFETY: see above.
    let offset = unsafe { core::ptr::read_volatile(ipc_buf.add(1)) };
    // SAFETY: see above.
    let max_len = unsafe { core::ptr::read_volatile(ipc_buf.add(2)) };

    if fd_idx >= MAX_FDS || !fds[fd_idx].in_use
    {
        let _ = syscall::ipc_reply(4, 0, &[]); // InvalidFd
        return;
    }

    let fd = &fds[fd_idx];
    let mut out = [0u8; SECTOR_SIZE];
    let bytes_read = read_file_data(
        fd.start_cluster,
        fd.file_size,
        offset,
        max_len,
        state,
        block_dev,
        ipc_buf,
        &mut out,
    );

    // Write reply: data[0] = bytes_read, data[1..] = file data.
    // SAFETY: IPC buffer is valid.
    unsafe { core::ptr::write_volatile(ipc_buf, bytes_read as u64) };

    // Pack file data into IPC buffer starting at word 1.
    let word_count = bytes_read.div_ceil(8);
    for i in 0..word_count
    {
        let base = i * 8;
        let mut word: u64 = 0;
        for j in 0..8
        {
            if base + j < bytes_read
            {
                word |= u64::from(out[base + j]) << (j * 8);
            }
        }
        // SAFETY: ipc_buf + 1 + i is within the IPC buffer page.
        unsafe { core::ptr::write_volatile(ipc_buf.add(1 + i), word) };
    }

    let _ = syscall::ipc_reply(0, 1 + word_count, &[]);
}

fn handle_close(fds: &mut [FatFd; MAX_FDS], ipc_buf: *mut u64)
{
    // SAFETY: IPC buffer is valid.
    let fd_idx = unsafe { core::ptr::read_volatile(ipc_buf) } as usize;

    if fd_idx >= MAX_FDS || !fds[fd_idx].in_use
    {
        let _ = syscall::ipc_reply(4, 0, &[]); // InvalidFd
        return;
    }

    fds[fd_idx].in_use = false;
    let _ = syscall::ipc_reply(0, 0, &[]);
}

fn handle_stat(fds: &[FatFd; MAX_FDS], ipc_buf: *mut u64)
{
    // SAFETY: IPC buffer is valid.
    let fd_idx = unsafe { core::ptr::read_volatile(ipc_buf) } as usize;

    if fd_idx >= MAX_FDS || !fds[fd_idx].in_use
    {
        let _ = syscall::ipc_reply(4, 0, &[]); // InvalidFd
        return;
    }

    let fd = &fds[fd_idx];
    let flags: u64 = u64::from(fd.is_dir) | 2; // bit 0=dir, bit 1=read-only

    // SAFETY: IPC buffer is valid.
    unsafe {
        core::ptr::write_volatile(ipc_buf, u64::from(fd.file_size));
        core::ptr::write_volatile(ipc_buf.add(1), flags);
    }
    let _ = syscall::ipc_reply(0, 2, &[]);
}

fn handle_readdir(state: &mut FatState, fds: &[FatFd; MAX_FDS], block_dev: u32, ipc_buf: *mut u64)
{
    // SAFETY: IPC buffer is valid.
    let fd_idx = unsafe { core::ptr::read_volatile(ipc_buf) } as usize;
    // SAFETY: see above.
    let entry_idx = unsafe { core::ptr::read_volatile(ipc_buf.add(1)) };

    if fd_idx >= MAX_FDS || !fds[fd_idx].in_use || !fds[fd_idx].is_dir
    {
        let _ = syscall::ipc_reply(4, 0, &[]); // InvalidFd
        return;
    }

    let dir_cluster = fds[fd_idx].start_cluster;

    let Some(entry) = read_dir_entry_at_index(dir_cluster, entry_idx, state, block_dev, ipc_buf)
    else
    {
        let _ = syscall::ipc_reply(LABEL_END_OF_DIR, 0, &[]);
        return;
    };

    // Format 8.3 name for reply: trim trailing spaces, insert dot.
    let mut name_buf = [0u8; 12];
    let name_len = format_83_name(&entry.name, &mut name_buf);
    let flags: u64 = u64::from(entry.attr & 0x10 != 0);

    // SAFETY: IPC buffer is valid.
    unsafe {
        core::ptr::write_volatile(ipc_buf, name_len as u64);
        core::ptr::write_volatile(ipc_buf.add(1), u64::from(entry.size));
        core::ptr::write_volatile(ipc_buf.add(2), flags);
    }

    // Pack name bytes into data[3..].
    let word_count = name_len.div_ceil(8);
    for i in 0..word_count
    {
        let base = i * 8;
        let mut word: u64 = 0;
        for j in 0..8
        {
            if base + j < name_len
            {
                word |= u64::from(name_buf[base + j]) << (j * 8);
            }
        }
        // SAFETY: ipc_buf + 3 + i is within the IPC buffer page.
        unsafe { core::ptr::write_volatile(ipc_buf.add(3 + i), word) };
    }

    let _ = syscall::ipc_reply(0, 3 + word_count, &[]);
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Read path bytes from IPC buffer data words.
fn read_path_from_ipc(ipc_buf: *const u64, path_len: usize, buf: &mut [u8; 48])
{
    let word_count = path_len.div_ceil(8).min(6);
    for i in 0..word_count
    {
        // SAFETY: ipc_buf is valid, i < MSG_DATA_WORDS_MAX.
        let word = unsafe { core::ptr::read_volatile(ipc_buf.add(i)) };
        let base = i * 8;
        let bytes = word.to_le_bytes();
        for j in 0..8
        {
            if base + j < path_len
            {
                buf[base + j] = bytes[j];
            }
        }
    }
}

/// Format an 8.3 directory entry name into a human-readable form.
///
/// Trims trailing spaces, inserts a dot between name and extension.
/// Returns the length of the formatted name.
fn format_83_name(raw: &[u8; 11], out: &mut [u8; 12]) -> usize
{
    let mut pos = 0;

    // Base name (first 8 chars, trim trailing spaces).
    let mut base_end = 8;
    while base_end > 0 && raw[base_end - 1] == b' '
    {
        base_end -= 1;
    }
    for &b in &raw[..base_end]
    {
        out[pos] = b;
        pos += 1;
    }

    // Extension (last 3 chars, trim trailing spaces).
    let mut ext_end = 11;
    while ext_end > 8 && raw[ext_end - 1] == b' '
    {
        ext_end -= 1;
    }
    if ext_end > 8
    {
        out[pos] = b'.';
        pos += 1;
        for &b in &raw[8..ext_end]
        {
            out[pos] = b;
            pos += 1;
        }
    }

    pos
}

fn idle_loop() -> !
{
    loop
    {
        let _ = syscall::thread_yield();
    }
}
