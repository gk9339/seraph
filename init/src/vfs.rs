// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// init/src/vfs.rs

//! VFS client operations for init.
//!
//! Provides helpers for mounting filesystems and reading files through the
//! VFS daemon (`vfsd`) via IPC. `OPEN` goes through vfsd for namespace
//! resolution; subsequent file operations (read/close) are performed
//! directly on the per-file capability returned by `OPEN`.

use crate::logging::log;
use ipc::{fs_labels, vfsd_labels};

// ── UUID parsing ─────────────────────────────────────────────────────────────

/// Parse `root=UUID=<uuid>` from kernel cmdline bytes.
///
/// UUID format: `12345678-abcd-ef01-2345-6789abcdef01` (36 chars).
/// Converts to 16-byte mixed-endian GPT format.
pub fn parse_root_uuid(cmdline: &[u8], out: &mut [u8; 16]) -> bool
{
    // Find "root=UUID=" in the cmdline.
    let prefix = b"root=UUID=";
    let mut start = None;
    for i in 0..cmdline.len().saturating_sub(prefix.len())
    {
        if &cmdline[i..i + prefix.len()] == prefix
        {
            start = Some(i + prefix.len());
            break;
        }
    }

    let Some(uuid_start) = start
    else
    {
        return false;
    };

    // Need at least 36 characters for a standard UUID string.
    if uuid_start + 36 > cmdline.len()
    {
        return false;
    }

    let uuid_str = &cmdline[uuid_start..uuid_start + 36];
    parse_uuid_to_gpt_bytes(uuid_str, out)
}

/// Parse a UUID string (36 bytes, e.g. `12345678-abcd-ef01-2345-6789abcdef01`)
/// into 16-byte mixed-endian GPT format.
///
/// GPT stores UUIDs with the first three groups byte-swapped (little-endian)
/// and the last two groups as-is (big-endian).
fn parse_uuid_to_gpt_bytes(s: &[u8], out: &mut [u8; 16]) -> bool
{
    // Parse hex string, skipping dashes.
    let mut hex = [0u8; 32];
    let mut hi = 0;
    for &b in s
    {
        if b == b'-'
        {
            continue;
        }
        if hi >= 32
        {
            return false;
        }
        let nibble = match b
        {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            _ => return false,
        };
        hex[hi] = nibble;
        hi += 1;
    }
    if hi != 32
    {
        return false;
    }

    // Assemble bytes from nibble pairs.
    let mut raw = [0u8; 16];
    for i in 0..16
    {
        raw[i] = (hex[i * 2] << 4) | hex[i * 2 + 1];
    }

    // Convert to mixed-endian GPT format:
    // Group 1 (bytes 0-3): little-endian u32
    out[0] = raw[3];
    out[1] = raw[2];
    out[2] = raw[1];
    out[3] = raw[0];
    // Group 2 (bytes 4-5): little-endian u16
    out[4] = raw[5];
    out[5] = raw[4];
    // Group 3 (bytes 6-7): little-endian u16
    out[6] = raw[7];
    out[7] = raw[6];
    // Groups 4-5 (bytes 8-15): big-endian (as-is)
    out[8..16].copy_from_slice(&raw[8..16]);

    true
}

// ── VFS IPC operations ──────────────────────────────────────────────────────

/// Send a MOUNT IPC request to vfsd.
///
/// MOUNT data layout: `data[0..2]` = UUID, `data[2]` = `path_len`,
/// `data[3..]` = path.
pub fn send_mount(vfsd_ep: u32, ipc_buf: *mut u64, uuid: &[u8; 16], path: &[u8]) -> bool
{
    // SAFETY: ipc_buf is the caller's registered IPC buffer page.
    let ipc = unsafe { ipc::IpcBuf::from_raw(ipc_buf) };
    // Pack UUID into data[0..2].
    let w0 = u64::from_le_bytes(uuid[..8].try_into().unwrap_or([0; 8]));
    let w1 = u64::from_le_bytes(uuid[8..].try_into().unwrap_or([0; 8]));
    ipc.write_word(0, w0);
    ipc.write_word(1, w1);
    ipc.write_word(2, path.len() as u64);

    // Pack path bytes into data[3..].
    let word_count = path.len().div_ceil(8).min(8);
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
        ipc.write_word(3 + i, word);
    }

    let total_words = 3 + word_count;
    let Ok((reply_label, _)) = syscall::ipc_call(vfsd_ep, vfsd_labels::MOUNT, total_words, &[])
    else
    {
        return false;
    };
    reply_label == 0
}

/// Read a file from the VFS into a buffer. Returns bytes read (0 on error).
///
/// Sends `OPEN` to vfsd for namespace resolution, receives a per-file
/// capability, then performs `READ` and `CLOSE` directly on the file cap.
// clippy::too_many_lines: vfs_read_file implements a fixed three-step VFS
// protocol — OPEN, READ (loop), CLOSE — that must run in one scope because
// the per-file capability returned by OPEN is the argument to READ and CLOSE.
// Splitting scatters the IPC-buffer packing/unpacking for the same ipc_buf
// across three helpers that each need the file_cap, the ipc_buf, and the
// reply-label branch handling; the linear presentation keeps the protocol
// sequence readable.
#[allow(clippy::too_many_lines)]
pub fn vfs_read_file(vfsd_ep: u32, ipc_buf: *mut u64, path: &[u8], buf: &mut [u8; 512]) -> usize
{
    // SAFETY: ipc_buf is the caller's registered IPC buffer page.
    let ipc = unsafe { ipc::IpcBuf::from_raw(ipc_buf) };
    // OPEN — send to vfsd for mount resolution.
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
        ipc.write_word(i, word);
    }

    let open_label = vfsd_labels::OPEN | ((path.len() as u64) << 16);
    let Ok((reply_label, _)) = syscall::ipc_call(vfsd_ep, open_label, word_count, &[])
    else
    {
        log("init: vfs_read: OPEN failed");
        return 0;
    };
    if reply_label != 0
    {
        log("init: vfs_read: OPEN error (not found?)");
        return 0;
    }

    // Read the per-file capability from the reply.
    // SAFETY: ipc_buf is the registered IPC buffer.
    #[allow(clippy::cast_ptr_alignment)]
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc_buf) };
    if cap_count == 0
    {
        log("init: vfs_read: OPEN returned no file cap");
        return 0;
    }
    let file_cap = reply_caps[0];

    // READ — call directly on the file cap (not vfsd).
    // New protocol: data[0] = offset, data[1] = max_len (no FD field).
    ipc.write_word(0, 0u64);
    ipc.write_word(1, 512u64);

    let Ok((reply_label, _)) = syscall::ipc_call(file_cap, fs_labels::FS_READ, 2, &[])
    else
    {
        log("init: vfs_read: READ failed");
        let _ = syscall::cap_delete(file_cap);
        return 0;
    };
    if reply_label != 0
    {
        log("init: vfs_read: READ error");
        let _ = syscall::cap_delete(file_cap);
        return 0;
    }

    // data[0] = bytes_read, data[1..] = content.
    // Copy BEFORE any log() calls (IPC buffer shared).
    let bytes_read = ipc.read_word(0) as usize;
    let content_words = bytes_read.div_ceil(8);
    for i in 0..content_words
    {
        let word = ipc.read_word(1 + i);
        let base = i * 8;
        for j in 0..8
        {
            if base + j < bytes_read && base + j < buf.len()
            {
                buf[base + j] = ((word >> (j * 8)) & 0xFF) as u8;
            }
        }
    }

    // CLOSE — call directly on the file cap.
    let _ = syscall::ipc_call(file_cap, fs_labels::FS_CLOSE, 0, &[]);
    let _ = syscall::cap_delete(file_cap);

    bytes_read
}

/// Parse `mounts.conf` and issue MOUNT requests for each entry.
///
/// Format: one mount per line, `UUID=<uuid> <path> <fstype>`.
/// Lines starting with `#` are comments. Empty lines are skipped.
pub fn process_mounts_conf(data: &[u8], vfsd_ep: u32, ipc_buf: *mut u64)
{
    let mut offset = 0;

    while offset < data.len()
    {
        // Find end of line.
        let line_end = data[offset..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(data.len(), |p| offset + p);
        let line = &data[offset..line_end];
        offset = line_end + 1;

        // Skip empty lines and comments.
        if line.is_empty() || line[0] == b'#'
        {
            continue;
        }

        // Trim trailing whitespace.
        let mut end = line.len();
        while end > 0 && (line[end - 1] == b' ' || line[end - 1] == b'\r')
        {
            end -= 1;
        }
        let line = &line[..end];

        // Parse: UUID=<uuid> <path> <fstype>
        if line.len() < 43 || &line[..5] != b"UUID="
        {
            // Unrecognised line — log and skip.
            log("init: mounts.conf: skipping unrecognised line");
            continue;
        }

        let uuid_str = &line[5..41]; // 36 chars
        let mut uuid = [0u8; 16];
        if !parse_uuid_to_gpt_bytes(uuid_str, &mut uuid)
        {
            log("init: mounts.conf: invalid UUID");
            continue;
        }

        // After UUID: " <path> <fstype>"
        let rest = &line[42..]; // skip "UUID=<36> "
        let space_pos = rest.iter().position(|&b| b == b' ');
        let mount_path = if let Some(sp) = space_pos
        {
            &rest[..sp]
        }
        else
        {
            rest // no fstype specified, just path
        };

        if send_mount(vfsd_ep, ipc_buf, &uuid, mount_path)
        {
            log("init: mounts.conf: mount ok");
        }
        else
        {
            log("init: mounts.conf: mount failed");
        }
    }
}
