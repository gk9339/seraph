// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// vfsd/src/gpt.rs

//! GPT partition table parsing.
//!
//! Reads the GUID Partition Table from a block device via IPC and populates
//! a fixed-size partition array with UUID and starting LBA for each entry.

/// Maximum GPT partitions we track.
pub const MAX_GPT_PARTS: usize = 8;

/// Sector size for block I/O.
const SECTOR_SIZE: usize = 512;

/// A discovered GPT partition (UUID + starting LBA).
pub struct GptEntry
{
    pub uuid: [u8; 16],
    pub first_lba: u64,
    pub active: bool,
}

impl GptEntry
{
    pub const fn empty() -> Self
    {
        Self {
            uuid: [0; 16],
            first_lba: 0,
            active: false,
        }
    }
}

/// Create a default GPT partition table with all entries inactive.
pub fn new_gpt_table() -> [GptEntry; MAX_GPT_PARTS]
{
    [
        GptEntry::empty(),
        GptEntry::empty(),
        GptEntry::empty(),
        GptEntry::empty(),
        GptEntry::empty(),
        GptEntry::empty(),
        GptEntry::empty(),
        GptEntry::empty(),
    ]
}

/// Read a single sector from the block device via IPC.
fn read_block_sector(
    blk_ep: u32,
    sector: u64,
    buf: &mut [u8; SECTOR_SIZE],
    ipc_buf: *mut u64,
) -> bool
{
    // SAFETY: ipc_buf is the registered IPC buffer page.
    let ipc = unsafe { ipc::IpcBuf::from_raw(ipc_buf) };
    ipc.write_word(0, sector);

    let Ok((reply_label, _)) = syscall::ipc_call(blk_ep, ipc::blk_labels::READ_BLOCK, 1, &[])
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
        let word = ipc.read_word(i);
        let base = i * 8;
        let bytes = word.to_le_bytes();
        buf[base..base + 8].copy_from_slice(&bytes);
    }

    true
}

/// GPT parsing error.
pub enum GptError
{
    /// Block I/O read failed when reading the GPT header.
    IoError,
    /// GPT header signature is not "EFI PART".
    InvalidSignature,
    /// Partition entry size is zero or exceeds sector size.
    InvalidEntrySize,
}

/// Parse the GPT and populate a partition table with UUID and LBA for
/// each non-empty partition. Returns the number of entries found, or an
/// error if the header cannot be read or validated.
// too_many_lines: sequential GPT header validation, sector iteration, and
// partition extraction that must share mutable state.
#[allow(clippy::too_many_lines)]
pub fn parse_gpt(
    blk_ep: u32,
    ipc_buf: *mut u64,
    parts: &mut [GptEntry; MAX_GPT_PARTS],
) -> Result<usize, GptError>
{
    let mut sector = [0u8; SECTOR_SIZE];

    // Read GPT header at LBA 1.
    if !read_block_sector(blk_ep, 1, &mut sector, ipc_buf)
    {
        return Err(GptError::IoError);
    }

    // Validate signature "EFI PART".
    if &sector[0..8] != b"EFI PART"
    {
        return Err(GptError::InvalidSignature);
    }

    let part_entry_lba = u64::from_le_bytes(sector[72..80].try_into().unwrap_or([0; 8]));
    let num_parts = u32::from_le_bytes(sector[80..84].try_into().unwrap_or([0; 4]));
    let entry_size = u32::from_le_bytes(sector[84..88].try_into().unwrap_or([0; 4]));

    if entry_size == 0 || entry_size > 512
    {
        return Err(GptError::InvalidEntrySize);
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
            runtime::log!("vfsd: GPT: partition at LBA {:#018x}", first_lba);
            found += 1;
            entries_checked += 1;
        }
    }

    runtime::log!("vfsd: GPT: partitions found: {}", found);
    Ok(found)
}

/// Look up a partition UUID in the GPT table. Returns the LBA offset or 0.
pub fn lookup_partition_by_uuid(uuid: &[u8; 16], parts: &[GptEntry; MAX_GPT_PARTS]) -> u64
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
