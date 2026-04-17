// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// procmgr/src/main.rs

//! Seraph process manager — IPC server for process lifecycle management.
//!
//! Receives requests via IPC to create, configure, and start new processes.
//! Supports both in-memory ELF loading from boot module frames and streaming
//! from the VFS. See `procmgr/docs/ipc-interface.md`.
//!
//! `CREATE_PROCESS` and `CREATE_FROM_VFS` accept the child's module source and
//! the caller's bootstrap endpoint (a tokened send cap); the endpoint is
//! installed in the child `CSpace` and recorded in `ProcessInfo` as the
//! `creator_endpoint_cap`. The child requests its initial cap set from the
//! caller over IPC at startup. procmgr itself has no knowledge of the child's
//! service-specific capabilities.

#![no_std]
#![no_main]
// cast_possible_truncation: targets 64-bit only; u64/usize conversions lossless.
#![allow(clippy::cast_possible_truncation)]

extern crate runtime;

mod frames;
mod loader;
mod process;

use frames::FramePool;
use ipc::{procmgr_errors, procmgr_labels, IpcBuf};
use process_abi::StartupInfo;

/// Init → procmgr bootstrap plan (one round):
///   caps[0]: service endpoint (procmgr receives requests on this)
///   data word 0: `memory_frame_base`
///   data word 1: `memory_frame_count`
fn bootstrap_from_init(creator_ep: u32, ipc: IpcBuf) -> Option<(u32, u32, u32)>
{
    if creator_ep == 0
    {
        return None;
    }
    let round = ipc::bootstrap::request_round(creator_ep, ipc).ok()?;
    if round.data_words < 2 || round.cap_count < 1 || !round.done
    {
        return None;
    }
    let service_ep = round.caps[0];
    let frame_base = ipc.read_word(0) as u32;
    let frame_count = ipc.read_word(1) as u32;
    Some((service_ep, frame_base, frame_count))
}

#[no_mangle]
extern "Rust" fn main(startup: &StartupInfo) -> !
{
    if syscall::ipc_buffer_set(startup.ipc_buffer as u64).is_err()
    {
        syscall::thread_exit();
    }

    let self_aspace = startup.self_aspace;
    // SAFETY: IPC buffer is page-aligned and registered.
    let ipc = unsafe { IpcBuf::from_bytes(startup.ipc_buffer) };
    let ipc_buf = ipc.as_ptr();

    // Bootstrap service endpoint + memory pool bounds from init.
    let Some((service_ep, frame_base, frame_count)) =
        bootstrap_from_init(startup.creator_endpoint, ipc)
    else
    {
        syscall::thread_exit();
    };

    let mut pool = FramePool::new(frame_base, frame_count);
    let mut table = process::ProcessTable::new();
    let mut vfsd_ep: u32 = 0;

    loop
    {
        let Ok((label, token)) = syscall::ipc_recv(service_ep)
        else
        {
            continue;
        };

        match label & 0xFFFF
        {
            procmgr_labels::CREATE_PROCESS =>
            {
                handle_create(ipc_buf, &mut pool, self_aspace, &mut table, service_ep);
            }

            procmgr_labels::START_PROCESS =>
            {
                // Token from ipc_recv identifies which process to start.
                match process::start_process(token, &mut table)
                {
                    Ok(()) =>
                    {
                        let _ = syscall::ipc_reply(procmgr_errors::SUCCESS, 0, &[]);
                    }
                    Err(code) =>
                    {
                        let _ = syscall::ipc_reply(code, 0, &[]);
                    }
                }
            }

            procmgr_labels::REQUEST_FRAMES =>
            {
                handle_request_frames(ipc_buf, &mut pool);
            }

            procmgr_labels::CREATE_FROM_VFS =>
            {
                handle_create_from_vfs(
                    label,
                    ipc_buf,
                    vfsd_ep,
                    &mut pool,
                    self_aspace,
                    &mut table,
                    service_ep,
                );
            }

            procmgr_labels::SET_VFSD_EP =>
            {
                // SAFETY: ipc_buf is the registered IPC buffer page.
                let (cap_count, caps) = unsafe { syscall::read_recv_caps(ipc_buf) };

                if cap_count > 0
                {
                    vfsd_ep = caps[0];
                    let _ = syscall::ipc_reply(procmgr_errors::SUCCESS, 0, &[]);
                }
                else
                {
                    let _ = syscall::ipc_reply(procmgr_errors::INVALID_ARGUMENT, 0, &[]);
                }
            }

            _ =>
            {
                let _ = syscall::ipc_reply(procmgr_errors::UNKNOWN_OPCODE, 0, &[]);
            }
        }
    }
}

/// Reply with a successful process creation result.
///
/// Reply caps: `[process_handle, thread]`.
fn reply_create_result(result: &process::CreateResult)
{
    let _ = syscall::ipc_reply(
        procmgr_errors::SUCCESS,
        0,
        &[result.process_handle, result.thread_for_caller],
    );
}

/// Handle `CREATE_PROCESS` — create a process from a boot module frame.
///
/// Expects `caps = [module_frame, creator_endpoint]`.
fn handle_create(
    ipc_buf: *mut u64,
    pool: &mut FramePool,
    self_aspace: u32,
    table: &mut process::ProcessTable,
    self_endpoint: u32,
)
{
    // SAFETY: ipc_buf is the registered IPC buffer page, page-aligned.
    let (cap_count, caps) = unsafe { syscall::read_recv_caps(ipc_buf) };

    if cap_count == 0
    {
        let _ = syscall::ipc_reply(procmgr_errors::INVALID_ELF, 0, &[]);
        return;
    }

    let module_cap = caps[0];
    let creator_ep = if cap_count >= 2 { caps[1] } else { 0 };

    match process::create_process(
        module_cap,
        pool,
        self_aspace,
        table,
        self_endpoint,
        creator_ep,
    )
    {
        Some(result) => reply_create_result(&result),
        None =>
        {
            let _ = syscall::ipc_reply(procmgr_errors::OUT_OF_MEMORY, 0, &[]);
        }
    }
}

/// Handle `REQUEST_FRAMES` — allocate and return physical memory frames.
fn handle_request_frames(ipc_buf: *mut u64, pool: &mut FramePool)
{
    // SAFETY: ipc_buf is the registered IPC buffer page.
    let ipc = unsafe { ipc::IpcBuf::from_raw(ipc_buf) };
    let requested = ipc.read_word(0);

    if requested == 0 || requested > 4
    {
        let _ = syscall::ipc_reply(procmgr_errors::INVALID_ARGUMENT, 0, &[]);
        return;
    }

    let mut caps = [0u32; 4];
    let mut granted: u64 = 0;

    for cap_slot in caps.iter_mut().take(requested as usize)
    {
        if let Some(page_cap) = pool.alloc_page()
        {
            *cap_slot = page_cap;
            granted += 1;
        }
        else
        {
            break;
        }
    }

    if granted == 0
    {
        let _ = syscall::ipc_reply(procmgr_errors::REQUEST_FRAMES_OOM, 0, &[]);
    }
    else
    {
        ipc.write_word(0, granted);
        let _ = syscall::ipc_reply(procmgr_errors::SUCCESS, 1, &caps[..granted as usize]);
    }
}

/// Handle `CREATE_FROM_VFS` — create a process from a VFS path.
///
/// Expects `caps = [creator_endpoint]` (module is loaded from VFS, not passed in).
#[allow(clippy::too_many_arguments)]
fn handle_create_from_vfs(
    label: u64,
    ipc_buf: *mut u64,
    vfsd_ep: u32,
    pool: &mut FramePool,
    self_aspace: u32,
    table: &mut process::ProcessTable,
    self_endpoint: u32,
)
{
    if vfsd_ep == 0
    {
        let _ = syscall::ipc_reply(procmgr_errors::NO_VFSD_ENDPOINT, 0, &[]);
        return;
    }

    let path_len = ((label >> 16) & 0xFFFF) as usize;
    if path_len == 0 || path_len > ipc::MAX_PATH_LEN
    {
        let _ = syscall::ipc_reply(procmgr_errors::FILE_NOT_FOUND, 0, &[]);
        return;
    }

    // SAFETY: ipc_buf is the registered IPC buffer page.
    let (cap_count, caps) = unsafe { syscall::read_recv_caps(ipc_buf) };
    let creator_ep = if cap_count >= 1 { caps[0] } else { 0 };

    let mut path_buf = [0u8; ipc::MAX_PATH_LEN];
    // SAFETY: ipc_buf is the registered IPC buffer page.
    let ipc_ref = unsafe { ipc::IpcBuf::from_raw(ipc_buf) };
    let effective_len = ipc::read_path_from_ipc(ipc_ref, path_len, &mut path_buf);

    match process::create_process_from_vfs(
        vfsd_ep,
        &path_buf[..effective_len],
        pool,
        self_aspace,
        table,
        ipc_buf,
        self_endpoint,
        creator_ep,
    )
    {
        Ok(result) => reply_create_result(&result),
        Err(code) =>
        {
            let _ = syscall::ipc_reply(code, 0, &[]);
        }
    }
}
