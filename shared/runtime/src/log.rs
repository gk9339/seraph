// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// shared/runtime/src/log.rs

//! IPC-based logging for userspace services.
//!
//! Services send log messages via synchronous IPC to a log endpoint (initially
//! serviced by init, later handed over to logd). The endpoint cap slot is set
//! once at startup via [`log_init`]; subsequent [`log`] calls serialise the
//! message bytes into the IPC buffer and call [`syscall::ipc_call`].
//!
//! Messages longer than 48 bytes are split across multiple IPC calls using
//! a continuation protocol encoded in the label field.
//!
//! # Label encoding
//!
//! - Bits 0-15: label ID ([`LOG_LABEL_BASE`] = 10)
//! - Bits 16-31: total message length in bytes
//! - Bit 32: continuation flag (1 = more chunks follow, 0 = final chunk)

use core::fmt;

use syscall_abi::MSG_DATA_WORDS_MAX;

/// Base label ID for log messages (bits 0-15).
pub const LOG_LABEL_BASE: u64 = 10;

/// Continuation flag (bit 32): set when more chunks follow.
pub const LOG_CONTINUATION: u64 = 1 << 32;

/// Maximum bytes per IPC chunk.
const CHUNK_SIZE: usize = MSG_DATA_WORDS_MAX * 8;

static mut LOG_ENDPOINT: u32 = 0;
static mut LOG_IPC_BUF: *mut u64 = core::ptr::null_mut();

/// Set the log endpoint cap slot and IPC buffer pointer.
///
/// Must be called once before any [`log`] calls. The `ipc_buf` must be the
/// same pointer registered with [`syscall::ipc_buffer_set`].
///
/// # Safety
///
/// Must be called from a single thread before any concurrent log calls.
pub unsafe fn log_init(endpoint_slot: u32, ipc_buf: *mut u8)
{
    // SAFETY: single-threaded init; caller guarantees exclusivity.
    unsafe {
        LOG_ENDPOINT = endpoint_slot;
        // cast_ptr_alignment: IPC buffer is page-aligned (4096-byte), satisfying u64 alignment.
        #[allow(clippy::cast_ptr_alignment)]
        {
            LOG_IPC_BUF = ipc_buf.cast::<u64>();
        }
    }
}

/// Send raw bytes to the log endpoint, splitting across multiple IPC calls
/// if the message exceeds [`CHUNK_SIZE`] bytes.
fn send_bytes(ep: u32, buf: *mut u64, bytes: &[u8])
{
    let total_len = bytes.len();
    if total_len == 0
    {
        return;
    }

    let mut offset = 0;
    while offset < total_len
    {
        let remaining = total_len - offset;
        let chunk_len = remaining.min(CHUNK_SIZE);
        let is_last = offset + chunk_len >= total_len;

        // Build label: base | (total_len << 16) | continuation flag.
        let label = LOG_LABEL_BASE
            | ((total_len as u64 & 0xFFFF) << 16)
            | if is_last { 0 } else { LOG_CONTINUATION };

        // Pack chunk bytes into IPC data words, zeroing unused bytes.
        let word_count = chunk_len.div_ceil(8);
        for i in 0..MSG_DATA_WORDS_MAX
        {
            let mut word: u64 = 0;
            if i < word_count
            {
                let start = offset + i * 8;
                for j in 0..8
                {
                    let idx = start + j;
                    if idx < offset + chunk_len
                    {
                        word |= u64::from(bytes[idx]) << (j * 8);
                    }
                }
            }
            // SAFETY: IPC buffer is valid; i < MSG_DATA_WORDS_MAX.
            unsafe { core::ptr::write_volatile(buf.add(i), word) };
        }

        let _ = syscall::ipc_call(ep, label, word_count, &[]);
        offset += chunk_len;
    }
}

/// Send a log message via IPC to the log endpoint.
///
/// If [`log_init`] has not been called (endpoint is 0), the call is a no-op.
pub fn log(msg: &str)
{
    // SAFETY: reading statics set during single-threaded init.
    let (ep, buf) = unsafe { (LOG_ENDPOINT, LOG_IPC_BUF) };
    if ep == 0 || buf.is_null()
    {
        return;
    }
    send_bytes(ep, buf, msg.as_bytes());
}

/// Stack buffer for formatting a log message before sending it via IPC.
///
/// Implements [`fmt::Write`] so that [`core::fmt::write`] can render a
/// format string into it. Bytes beyond 256 are silently dropped.
struct LogBuffer
{
    buf: [u8; 256],
    pos: usize,
}

impl LogBuffer
{
    fn new() -> Self
    {
        Self {
            buf: [0u8; 256],
            pos: 0,
        }
    }
}

impl fmt::Write for LogBuffer
{
    fn write_str(&mut self, s: &str) -> fmt::Result
    {
        let bytes = s.as_bytes();
        let available = self.buf.len().saturating_sub(self.pos);
        let to_copy = bytes.len().min(available);
        self.buf[self.pos..self.pos + to_copy].copy_from_slice(&bytes[..to_copy]);
        self.pos += to_copy;
        Ok(())
    }
}

/// Format a log message and send it as a single IPC call to the log endpoint.
///
/// Called by the [`log!`] macro. If [`log_init`] has not been called
/// (endpoint is 0), the call is a no-op.
pub fn _log_fmt(args: fmt::Arguments)
{
    // SAFETY: reading statics set during single-threaded init.
    let (ep, buf) = unsafe { (LOG_ENDPOINT, LOG_IPC_BUF) };
    if ep == 0 || buf.is_null()
    {
        return;
    }
    let mut log_buf = LogBuffer::new();
    // Ignore formatting errors — best-effort logging; send whatever was written.
    let _ = fmt::write(&mut log_buf, args);
    send_bytes(ep, buf, &log_buf.buf[..log_buf.pos]);
}

/// Send a log message with a hex value suffix.
///
/// Outputs `prefix` followed by the hex representation of `val`.
pub fn log_hex(prefix: &str, val: u64)
{
    // SAFETY: reading statics set during single-threaded init.
    let (ep, buf) = unsafe { (LOG_ENDPOINT, LOG_IPC_BUF) };
    if ep == 0 || buf.is_null()
    {
        return;
    }

    // Build the message in a stack buffer.
    let prefix_bytes = prefix.as_bytes();
    // 256 bytes: enough for any reasonable prefix + "0x" + 16 hex digits.
    let mut msg_buf = [0u8; 256];
    let mut pos = 0;

    for &b in prefix_bytes
    {
        if pos >= msg_buf.len() - 18
        {
            break;
        }
        msg_buf[pos] = b;
        pos += 1;
    }

    msg_buf[pos] = b'0';
    pos += 1;
    msg_buf[pos] = b'x';
    pos += 1;

    for i in (0..16).rev()
    {
        let nibble = ((val >> (i * 4)) & 0xF) as u8;
        msg_buf[pos] = if nibble < 10
        {
            b'0' + nibble
        }
        else
        {
            b'a' + nibble - 10
        };
        pos += 1;
    }

    send_bytes(ep, buf, &msg_buf[..pos]);
}

/// Send a formatted log message via IPC to the log endpoint.
///
/// Accepts the same format string syntax as [`println!`]. The entire
/// formatted message is assembled in a 256-byte stack buffer and sent as
/// a single IPC call, so it appears as one line on the receiving end.
///
/// If [`log_init`] has not been called, the call is a no-op.
///
/// # Examples
///
/// ```ignore
/// runtime::log!("device found: {}", name);
/// runtime::log!("base address: {:#x}", addr);
/// ```
#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {
        $crate::log::_log_fmt(core::format_args!($($arg)*))
    };
}
