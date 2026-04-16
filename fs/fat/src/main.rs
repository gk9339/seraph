// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// fs/fat/src/main.rs

//! Seraph FAT filesystem driver.
//!
//! Implements read-only FAT16/FAT32 filesystem support. Receives IPC requests
//! from vfsd (mount/open) and directly from clients (read/close/stat/readdir)
//! conforming to `fs/docs/fs-driver-protocol.md`. All disk I/O is performed
//! via the block device IPC endpoint received at creation time.
//!
//! File identification uses capability tokens: `FS_OPEN` derives a tokened
//! send cap from the service endpoint and returns it to the caller. Clients
//! call file operations directly on the tokened cap; the token delivered by
//! `ipc_recv` identifies the open file.

#![no_std]
#![no_main]
#![allow(clippy::cast_possible_truncation)]

extern crate runtime;

mod bpb;
mod dir;
mod fat;
mod file;

use process_abi::{CapType, StartupInfo};

use bpb::{FatState, SECTOR_SIZE};
use dir::{format_83_name, read_dir_entry_at_index, resolve_path};
use fat::read_file_data;
use file::{OpenFile, MAX_OPEN_FILES};
use ipc::fs_labels;

/// Monotonic counter for token allocation. Starts at 1 (0 = untokened).
static mut NEXT_TOKEN: u64 = 1;

// ── Cap classification ─────────────────────────────────────────────────────

struct FatCaps
{
    block_dev: u32,
    log_sink: u32,
    service: u32,
}

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
            if d.aux0 == ipc::LOG_ENDPOINT_SENTINEL
            {
                caps.log_sink = d.slot;
            }
            else if d.aux0 == ipc::SERVICE_ENDPOINT_SENTINEL
            {
                caps.service = d.slot;
            }
            else if d.aux0 == ipc::BLOCK_ENDPOINT_SENTINEL
            {
                caps.block_dev = d.slot;
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
    let mut files = [
        OpenFile::empty(),
        OpenFile::empty(),
        OpenFile::empty(),
        OpenFile::empty(),
        OpenFile::empty(),
        OpenFile::empty(),
        OpenFile::empty(),
        OpenFile::empty(),
    ];

    service_loop(&caps, &mut state, &mut files, ipc_buf);
}

// ── Service loop ───────────────────────────────────────────────────────────

/// Main FAT service loop.
///
/// Dispatches on the token delivered by `ipc_recv`:
/// - token == 0: service-level request from vfsd (`FS_MOUNT`, `FS_OPEN`)
/// - token != 0: per-file request from a client (`FS_READ`, `FS_CLOSE`,
///   `FS_STAT`, `FS_READDIR`), identified by the token
fn service_loop(
    caps: &FatCaps,
    state: &mut FatState,
    files: &mut [OpenFile; MAX_OPEN_FILES],
    ipc_buf: *mut u64,
) -> !
{
    let mut mounted = false;

    loop
    {
        let Ok((label, token)) = syscall::ipc_recv(caps.service)
        else
        {
            continue;
        };

        let opcode = label & 0xFFFF;

        if token == 0
        {
            // Service-level request from vfsd (untokened cap).
            match opcode
            {
                fs_labels::FS_MOUNT =>
                {
                    if mounted
                    {
                        let _ = syscall::ipc_reply(0, 0, &[]);
                        continue;
                    }
                    handle_mount(caps.block_dev, state, ipc_buf);
                    mounted = true;
                }
                fs_labels::FS_OPEN if mounted =>
                {
                    handle_open(label, state, files, caps, ipc_buf);
                }
                _ =>
                {
                    let _ = syscall::ipc_reply(0xFF, 0, &[]);
                }
            }
        }
        else if mounted
        {
            // Per-file request from client (tokened cap).
            match opcode
            {
                fs_labels::FS_READ => handle_read(token, state, files, caps.block_dev, ipc_buf),
                fs_labels::FS_CLOSE => handle_close(token, files),
                fs_labels::FS_STAT => handle_stat(token, files, ipc_buf),
                fs_labels::FS_READDIR =>
                {
                    handle_readdir(token, state, files, caps.block_dev, ipc_buf);
                }
                _ =>
                {
                    let _ = syscall::ipc_reply(0xFF, 0, &[]);
                }
            }
        }
        else
        {
            let _ = syscall::ipc_reply(0xFF, 0, &[]);
        }
    }
}

// ── Operation handlers ─────────────────────────────────────────────────────

fn handle_mount(block_dev: u32, state: &mut FatState, ipc_buf: *mut u64)
{
    // SAFETY: IPC buffer is valid.
    let partition_offset = unsafe { core::ptr::read_volatile(ipc_buf) };
    state.partition_offset = partition_offset;

    let mut sector_buf = [0u8; SECTOR_SIZE];
    if !fat::read_sector(block_dev, 0, &mut sector_buf, ipc_buf, partition_offset)
    {
        runtime::log!("fatfs: failed to read partition sector 0");
        let _ = syscall::ipc_reply(2, 0, &[]); // IoError
        return;
    }

    if !bpb::parse_bpb(&sector_buf, state)
    {
        let _ = syscall::ipc_reply(1, 0, &[]); // InvalidFilesystem
        return;
    }

    runtime::log!("fatfs: filesystem mounted");
    let _ = syscall::ipc_reply(0, 0, &[]);
}

/// Handle `FS_OPEN`: resolve path, allocate file slot, derive a tokened
/// send cap from the service endpoint, and return it in the reply cap slot.
fn handle_open(
    label: u64,
    state: &mut FatState,
    files: &mut [OpenFile; MAX_OPEN_FILES],
    caps: &FatCaps,
    ipc_buf: *mut u64,
)
{
    let path_len = ((label >> 16) & 0xFFFF) as usize;
    if path_len == 0 || path_len > ipc::MAX_PATH_LEN
    {
        let _ = syscall::ipc_reply(1, 0, &[]); // NotFound
        return;
    }

    let mut path_buf = [0u8; ipc::MAX_PATH_LEN];
    // SAFETY: ipc_buf points to a valid IPC buffer with path data words.
    unsafe { ipc::read_path_from_ipc(ipc_buf.cast_const(), path_len, &mut path_buf) };
    let path = &path_buf[..path_len];

    let Some(entry) = resolve_path(path, state, caps.block_dev, ipc_buf)
    else
    {
        let _ = syscall::ipc_reply(1, 0, &[]); // NotFound
        return;
    };

    let Some(slot_idx) = file::alloc_slot(files)
    else
    {
        let _ = syscall::ipc_reply(3, 0, &[]); // TooManyOpen
        return;
    };

    // SAFETY: single-threaded; NEXT_TOKEN is only accessed in this function.
    let token = unsafe {
        let t = NEXT_TOKEN;
        NEXT_TOKEN += 1;
        t
    };

    // Derive a tokened send cap from our service endpoint.
    let Ok(file_cap) = syscall::cap_derive_token(caps.service, syscall_abi::RIGHTS_SEND, token)
    else
    {
        let _ = syscall::ipc_reply(2, 0, &[]); // OutOfMemory (cap derivation failed)
        return;
    };

    files[slot_idx] = OpenFile {
        token,
        start_cluster: entry.cluster,
        file_size: entry.size,
        is_dir: entry.attr & 0x10 != 0,
    };

    // Reply with the file cap — no data words needed.
    let _ = syscall::ipc_reply(0, 0, &[file_cap]);
}

/// Handle `FS_READ`: token identifies the file. Data layout:
/// request `data[0]` = offset, `data[1]` = `max_len`.
fn handle_read(
    token: u64,
    state: &mut FatState,
    files: &[OpenFile; MAX_OPEN_FILES],
    block_dev: u32,
    ipc_buf: *mut u64,
)
{
    let Some(idx) = file::find_by_token(files, token)
    else
    {
        let _ = syscall::ipc_reply(4, 0, &[]); // InvalidToken
        return;
    };

    // SAFETY: IPC buffer is valid and word-aligned.
    let offset = unsafe { core::ptr::read_volatile(ipc_buf) };
    // SAFETY: see above.
    let max_len = unsafe { core::ptr::read_volatile(ipc_buf.add(1)) };

    let file = &files[idx];
    let mut out = [0u8; SECTOR_SIZE];
    let bytes_read = read_file_data(
        file.start_cluster,
        file.file_size,
        offset,
        max_len,
        state,
        block_dev,
        ipc_buf,
        &mut out,
    );

    // SAFETY: IPC buffer is valid.
    unsafe { core::ptr::write_volatile(ipc_buf, bytes_read as u64) };

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

/// Handle `FS_CLOSE`: token identifies the file. No data words.
fn handle_close(token: u64, files: &mut [OpenFile; MAX_OPEN_FILES])
{
    let Some(idx) = file::find_by_token(files, token)
    else
    {
        let _ = syscall::ipc_reply(4, 0, &[]); // InvalidToken
        return;
    };

    files[idx] = OpenFile::empty();
    let _ = syscall::ipc_reply(0, 0, &[]);
}

/// Handle `FS_STAT`: token identifies the file. No data words in request.
fn handle_stat(token: u64, files: &[OpenFile; MAX_OPEN_FILES], ipc_buf: *mut u64)
{
    let Some(idx) = file::find_by_token(files, token)
    else
    {
        let _ = syscall::ipc_reply(4, 0, &[]); // InvalidToken
        return;
    };

    let file = &files[idx];
    let flags: u64 = u64::from(file.is_dir) | 2; // bit 0=dir, bit 1=read-only

    // SAFETY: IPC buffer is valid.
    unsafe {
        core::ptr::write_volatile(ipc_buf, u64::from(file.file_size));
        core::ptr::write_volatile(ipc_buf.add(1), flags);
    }
    let _ = syscall::ipc_reply(0, 2, &[]);
}

/// Handle `FS_READDIR`: token identifies the directory. `data[0]` = `entry_idx`.
fn handle_readdir(
    token: u64,
    state: &mut FatState,
    files: &[OpenFile; MAX_OPEN_FILES],
    block_dev: u32,
    ipc_buf: *mut u64,
)
{
    let Some(idx) = file::find_by_token(files, token)
    else
    {
        let _ = syscall::ipc_reply(4, 0, &[]); // InvalidToken
        return;
    };

    if !files[idx].is_dir
    {
        let _ = syscall::ipc_reply(4, 0, &[]); // InvalidToken (not a directory)
        return;
    }

    // SAFETY: IPC buffer is valid.
    let entry_idx = unsafe { core::ptr::read_volatile(ipc_buf) };
    let dir_cluster = files[idx].start_cluster;

    let Some(entry) = read_dir_entry_at_index(dir_cluster, entry_idx, state, block_dev, ipc_buf)
    else
    {
        let _ = syscall::ipc_reply(fs_labels::END_OF_DIR, 0, &[]);
        return;
    };

    let mut name_buf = [0u8; 12];
    let name_len = format_83_name(&entry.name, &mut name_buf);
    let flags: u64 = u64::from(entry.attr & 0x10 != 0);

    // SAFETY: IPC buffer is valid.
    unsafe {
        core::ptr::write_volatile(ipc_buf, name_len as u64);
        core::ptr::write_volatile(ipc_buf.add(1), u64::from(entry.size));
        core::ptr::write_volatile(ipc_buf.add(2), flags);
    }

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

// ── Helpers ────────────────────────────────────────────────────────────────

fn idle_loop() -> !
{
    loop
    {
        let _ = syscall::thread_yield();
    }
}
