// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// fs/fat/src/fat.rs

//! Block I/O, FAT chain traversal, and file data reading.
//!
//! Provides sector-level reads via the block device IPC endpoint, FAT table
//! lookups with single-sector caching, and cluster-chain-walking file reads.

use ipc::blk_labels;

use crate::bpb::{FatState, FatType, SECTOR_SIZE};

/// Read a single 512-byte sector from the block device into `buf`.
///
/// `sector` is partition-relative. The partition offset is added before
/// issuing the block read to translate to an absolute disk LBA.
pub fn read_sector(
    block_dev: u32,
    sector: u64,
    buf: &mut [u8; SECTOR_SIZE],
    ipc_buf: *mut u64,
    partition_offset: u64,
) -> bool
{
    // SAFETY: ipc_buf is the registered IPC buffer page.
    let ipc = unsafe { ipc::IpcBuf::from_raw(ipc_buf) };
    ipc.write_word(0, sector + partition_offset);

    let Ok((reply_label, _data_count)) =
        syscall::ipc_call(block_dev, blk_labels::READ_BLOCK, 1, &[])
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
        let word = ipc.read_word(i);
        let base = i * 8;
        let bytes = word.to_le_bytes();
        buf[base..base + 8].copy_from_slice(&bytes);
    }

    true
}

/// Look up the next cluster in the FAT chain.
///
/// Returns `Some(next)` for a valid next cluster, `None` for end-of-chain
/// or bad cluster.
#[allow(clippy::too_many_lines)]
// Justification: FAT16/FAT32 branches share structure but differ in entry
// widths and end-of-chain markers; extracting sub-functions would obscure
// the symmetry.
pub fn next_cluster(
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

/// Read up to 512 bytes from a file at a given byte offset.
///
/// Walks the cluster chain to find the correct cluster, reads the
/// relevant sector, and extracts the requested bytes.
///
/// Returns the number of bytes read (written to `out`).
#[allow(clippy::too_many_arguments)]
pub fn read_file_data(
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
