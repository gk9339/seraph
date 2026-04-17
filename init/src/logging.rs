// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// init/src/logging.rs

//! Logging subsystem for init.
//!
//! Provides serial-based early logging and IPC-based logging through a
//! dedicated log thread. The main thread switches from serial to IPC once
//! the log thread is running.

use crate::arch;
use crate::{FrameAlloc, PAGE_SIZE};
use init_protocol::InitInfo;

// ── Constants ────────────────────────────────────────────────────────────────

/// Virtual address for the log thread's IPC buffer (separate from main thread).
const LOG_THREAD_IPC_BUF_VA: u64 = 0x0000_0000_C000_1000; // main IPC buf + 1 page

/// Virtual address for the log thread's stack base.
const LOG_THREAD_STACK_VA: u64 = 0x0000_0000_D000_0000;

/// Number of stack pages for the log thread (16 KiB).
const LOG_THREAD_STACK_PAGES: u64 = 4;

/// Log label base (bits 0-15). Must match `runtime::log::LOG_LABEL_BASE`.
pub(crate) const LOG_LABEL_BASE: u64 = 10;

/// Continuation flag (bit 32). Must match `runtime::log::LOG_CONTINUATION`.
pub(crate) const LOG_CONTINUATION: u64 = 1 << 32;

/// Max bytes per IPC chunk. Must match `MSG_DATA_WORDS_MAX * 8` from `syscall_abi`.
const LOG_CHUNK_SIZE: usize = syscall_abi::MSG_DATA_WORDS_MAX * 8;

/// Maximum assembled log message length (multiple chunks).
const LOG_MAX_ASSEMBLED: usize = 256;

// ── Mutable state ────────────────────────────────────────────────────────────

/// Log endpoint cap slot for IPC-based logging (set after log thread starts).
static mut LOG_EP_SLOT: u32 = 0;

/// IPC buffer pointer for the main thread (set after IPC buffer is mapped).
static mut MAIN_IPC_BUF: *mut u64 = core::ptr::null_mut();

// ── Public interface ─────────────────────────────────────────────────────────

/// Log a message. Uses direct serial before the log thread is running,
/// then switches to IPC-based logging through the log thread.
pub fn log(s: &str)
{
    // SAFETY: LOG_EP_SLOT and MAIN_IPC_BUF are written once by the main thread
    // before any IPC log calls; log thread only reads its own log_ep argument.
    let log_ep = unsafe { LOG_EP_SLOT };
    // SAFETY: see above.
    let ipc_buf = unsafe { MAIN_IPC_BUF };

    if log_ep != 0 && !ipc_buf.is_null()
    {
        ipc_log(log_ep, ipc_buf, s);
    }
    else
    {
        serial_log(s);
    }
}

/// Switch the main thread from direct serial to IPC-based logging.
///
/// Must be called exactly once after the log thread has started.
pub fn set_ipc_logging(log_ep: u32, ipc_buf: *mut u64)
{
    // SAFETY: single main thread; log thread only reads its own log_ep argument.
    unsafe {
        LOG_EP_SLOT = log_ep;
        MAIN_IPC_BUF = ipc_buf;
    }
}

// ── Serial output ────────────────────────────────────────────────────────────

/// Direct serial output (early boot, before log thread exists).
pub(crate) fn serial_log(s: &str)
{
    for &b in s.as_bytes()
    {
        if b == b'\n'
        {
            arch::current::serial_write_byte(b'\r');
        }
        arch::current::serial_write_byte(b);
    }
    arch::current::serial_write_byte(b'\r');
    arch::current::serial_write_byte(b'\n');
}

// ── IPC-based logging ────────────────────────────────────────────────────────

/// IPC-based logging through the log thread.
///
/// Matches the protocol in `runtime::log`: label = `LOG_LABEL_BASE` | (len << 16),
/// data words = packed bytes, up to 48 bytes per chunk.
fn ipc_log(log_ep: u32, ipc_buf: *mut u64, s: &str)
{
    // SAFETY: ipc_buf is MAIN_IPC_BUF, set from the registered IPC buffer page.
    let ipc = unsafe { ipc::IpcBuf::from_raw(ipc_buf) };
    let bytes = s.as_bytes();
    let total_len = bytes.len();
    let mut offset = 0;

    while offset < total_len || total_len == 0
    {
        let remaining = total_len - offset;
        let chunk_len = remaining.min(LOG_CHUNK_SIZE);
        let is_last = offset + chunk_len >= total_len;

        // Pack bytes into IPC buffer data words.
        let word_count = chunk_len.div_ceil(8);
        for i in 0..word_count
        {
            let mut word: u64 = 0;
            let base = i * 8;
            for j in 0..8
            {
                let idx = offset + base + j;
                if idx < total_len
                {
                    word |= u64::from(bytes[idx]) << (j * 8);
                }
            }
            ipc.write_word(i, word);
        }

        let mut label = LOG_LABEL_BASE | ((total_len as u64) << 16);
        if !is_last
        {
            label |= LOG_CONTINUATION;
        }

        let _ = syscall::ipc_call(log_ep, label, word_count, &[]);

        offset += chunk_len;
        if total_len == 0
        {
            break;
        }
    }
}

// ── Log thread ───────────────────────────────────────────────────────────────

/// Spawn a dedicated log-receiving thread so the main thread can continue
/// bootstrap orchestration (making IPC calls to vfsd etc.) without blocking
/// service log output.
pub fn spawn_log_thread(info: &InitInfo, alloc: &mut FrameAlloc, log_ep: u32, ioport_cap: u32)
{
    // Allocate stack pages for the log thread.
    for i in 0..LOG_THREAD_STACK_PAGES
    {
        let Some(frame) = alloc.alloc_page()
        else
        {
            log("init: FATAL: cannot allocate log thread stack");
            syscall::thread_exit();
        };
        let Ok(rw_cap) = syscall::cap_derive(frame, syscall::RIGHTS_MAP_RW)
        else
        {
            log("init: FATAL: cannot derive log thread stack cap");
            syscall::thread_exit();
        };
        if syscall::mem_map(
            rw_cap,
            info.aspace_cap,
            LOG_THREAD_STACK_VA + i * PAGE_SIZE,
            0,
            1,
            0,
        )
        .is_err()
        {
            log("init: FATAL: cannot map log thread stack");
            syscall::thread_exit();
        }
    }

    // Allocate IPC buffer page for the log thread.
    let Some(ipc_frame) = alloc.alloc_page()
    else
    {
        log("init: FATAL: cannot allocate log thread IPC buffer");
        syscall::thread_exit();
    };
    let Ok(ipc_rw_cap) = syscall::cap_derive(ipc_frame, syscall::RIGHTS_MAP_RW)
    else
    {
        log("init: FATAL: cannot derive log thread IPC cap");
        syscall::thread_exit();
    };
    if syscall::mem_map(ipc_rw_cap, info.aspace_cap, LOG_THREAD_IPC_BUF_VA, 0, 1, 0).is_err()
    {
        log("init: FATAL: cannot map log thread IPC buffer");
        syscall::thread_exit();
    }
    // Zero the IPC buffer.
    // SAFETY: LOG_THREAD_IPC_BUF_VA is mapped writable and covers one page.
    unsafe { core::ptr::write_bytes(LOG_THREAD_IPC_BUF_VA as *mut u8, 0, PAGE_SIZE as usize) };

    // Create the thread bound to init's address space and CSpace.
    let Ok(thread_cap) = syscall::cap_create_thread(info.aspace_cap, info.cspace_cap)
    else
    {
        log("init: FATAL: cannot create log thread");
        syscall::thread_exit();
    };

    // Bind I/O ports to the log thread so it can write to the serial port.
    // On x86-64, I/O port access is per-thread via the TSS IOPB.
    if ioport_cap != 0 && syscall::ioport_bind(thread_cap, ioport_cap).is_err()
    {
        log("init: log thread: ioport_bind failed");
    }

    let stack_top = LOG_THREAD_STACK_VA + LOG_THREAD_STACK_PAGES * PAGE_SIZE;

    // Pack log_ep (u32) and IPC buffer VA into the arg passed to the thread.
    // Low 32 bits = log_ep, high 32 bits unused (IPC buf VA is a known constant).
    let arg = u64::from(log_ep);

    if syscall::thread_configure(
        thread_cap,
        log_thread_entry as *const () as u64,
        stack_top,
        arg,
    )
    .is_err()
    {
        log("init: FATAL: cannot configure log thread");
        syscall::thread_exit();
    }
    if syscall::thread_start(thread_cap).is_err()
    {
        log("init: FATAL: cannot start log thread");
        syscall::thread_exit();
    }
}

/// Entry point for the log thread. Registers its own IPC buffer then enters
/// the log receive loop. Never returns.
///
/// Called via `thread_configure` with `arg` = log endpoint cap slot.
extern "C" fn log_thread_entry(arg: u64) -> !
{
    // Register this thread's IPC buffer.
    if syscall::ipc_buffer_set(LOG_THREAD_IPC_BUF_VA).is_err()
    {
        serial_log("init: log thread: ipc_buffer_set failed");
        syscall::thread_exit();
    }

    let log_ep = arg as u32;

    // SAFETY: LOG_THREAD_IPC_BUF_VA is the registered IPC buffer, page-aligned.
    #[allow(clippy::cast_ptr_alignment)]
    let ipc_buf = LOG_THREAD_IPC_BUF_VA as *mut u64;

    log_receive_loop(log_ep, ipc_buf);
}

// ── Log receive loop ─────────────────────────────────────────────────────────

/// Receive log messages from services and write them to serial.
///
/// Handles multi-chunk messages: chunks with the continuation flag set are
/// accumulated into `assembled_buf`. When the final chunk arrives (no flag),
/// the complete message is written to serial with CRLF.
fn log_receive_loop(log_ep: u32, ipc_buf_raw: *mut u64) -> !
{
    // SAFETY: ipc_buf_raw is the log thread's registered IPC buffer page.
    let ipc = unsafe { ipc::IpcBuf::from_raw(ipc_buf_raw) };
    let mut assembled_buf = [0u8; LOG_MAX_ASSEMBLED];
    let mut assembled_len: usize = 0;

    loop
    {
        let Ok((label, _data_count)) = syscall::ipc_recv(log_ep)
        else
        {
            continue;
        };

        let label_id = label & 0xFFFF;
        let total_len = ((label >> 16) & 0xFFFF) as usize;
        let has_continuation = label & LOG_CONTINUATION != 0;

        if label_id == LOG_LABEL_BASE
        {
            // Read chunk bytes from IPC buffer.
            let chunk_bytes = LOG_CHUNK_SIZE.min(total_len - assembled_len);
            let word_count = chunk_bytes.div_ceil(8);

            for i in 0..word_count
            {
                let word = ipc.read_word(i);
                let base = i * 8;
                for j in 0..8
                {
                    let idx = assembled_len + base + j;
                    if idx < LOG_MAX_ASSEMBLED && base + j < chunk_bytes
                    {
                        assembled_buf[idx] = ((word >> (j * 8)) & 0xFF) as u8;
                    }
                }
            }
            assembled_len += chunk_bytes;

            if !has_continuation
            {
                // Final chunk — print the complete message.
                let len = assembled_len.min(total_len).min(LOG_MAX_ASSEMBLED);
                for &b in &assembled_buf[..len]
                {
                    if b == b'\n'
                    {
                        arch::current::serial_write_byte(b'\r');
                    }
                    arch::current::serial_write_byte(b);
                }
                arch::current::serial_write_byte(b'\r');
                arch::current::serial_write_byte(b'\n');
                assembled_len = 0;
            }
        }

        // Reply to unblock the sender.
        let _ = syscall::ipc_reply(0, 0, &[]);
    }
}
