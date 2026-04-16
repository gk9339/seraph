// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// procmgr/src/main.rs

//! Seraph process manager — IPC server for process lifecycle management.
//!
//! Receives requests via IPC to create, configure, and start new processes.
//! Supports both in-memory ELF loading from boot module frames and streaming
//! from the VFS. See `procmgr/docs/ipc-interface.md`.

#![no_std]
#![no_main]
// cast_possible_truncation: targets 64-bit only; u64/usize conversions lossless.
#![allow(clippy::cast_possible_truncation)]

extern crate runtime;

mod frames;
mod loader;
mod process;

use frames::FramePool;
use ipc::procmgr_labels;
use process_abi::{ProcessInfo, StartupInfo, PROCESS_INFO_VADDR};

#[no_mangle]
extern "Rust" fn main(startup: &StartupInfo) -> !
{
    if syscall::ipc_buffer_set(startup.ipc_buffer as u64).is_err()
    {
        syscall::thread_exit();
    }

    let endpoint = startup.creator_endpoint;
    let self_aspace = startup.self_aspace;
    // cast_ptr_alignment: IPC buffer is page-aligned (4096), exceeding u64 alignment.
    #[allow(clippy::cast_ptr_alignment)]
    let ipc_buf = startup.ipc_buffer.cast::<u64>();

    // SAFETY: PROCESS_INFO_VADDR is mapped read-only by init.
    // cast_ptr_alignment: PROCESS_INFO_VADDR is page-aligned.
    #[allow(clippy::cast_ptr_alignment)]
    let proc_info = unsafe { &*(PROCESS_INFO_VADDR as *const ProcessInfo) };
    let mut pool = FramePool::new(proc_info.initial_caps_base, proc_info.initial_caps_count);
    let mut table = process::ProcessTable::new();
    let mut vfsd_ep: u32 = 0;

    loop
    {
        let Ok((label, token)) = syscall::ipc_recv(endpoint)
        else
        {
            continue;
        };

        match label & 0xFFFF
        {
            procmgr_labels::CREATE_PROCESS =>
            {
                handle_create(ipc_buf, &mut pool, self_aspace, &mut table, endpoint);
            }

            procmgr_labels::START_PROCESS =>
            {
                // Token from ipc_recv identifies which process to start.
                match process::start_process(token, &mut table)
                {
                    Ok(()) =>
                    {
                        let _ = syscall::ipc_reply(0, 0, &[]);
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
                    endpoint,
                );
            }

            procmgr_labels::SET_VFSD_EP =>
            {
                // SAFETY: ipc_buf is the registered IPC buffer page.
                #[allow(clippy::cast_ptr_alignment)]
                let (cap_count, caps) = unsafe { syscall::read_recv_caps(ipc_buf) };

                if cap_count > 0
                {
                    vfsd_ep = caps[0];
                    let _ = syscall::ipc_reply(0, 0, &[]);
                }
                else
                {
                    let _ = syscall::ipc_reply(1, 0, &[]);
                }
            }

            _ =>
            {
                let _ = syscall::ipc_reply(0xFFFF, 0, &[]);
            }
        }
    }
}

/// Reply with a successful process creation result.
///
/// Reply caps: `[process_handle, cspace, pi_frame, thread]`.
fn reply_create_result(result: &process::CreateResult)
{
    let _ = syscall::ipc_reply(
        0,
        0,
        &[
            result.process_handle,
            result.cspace_for_caller,
            result.pi_frame_for_caller,
            result.thread_for_caller,
        ],
    );
}

/// Handle `CREATE_PROCESS` — create a process from a boot module frame.
fn handle_create(
    ipc_buf: *mut u64,
    pool: &mut FramePool,
    self_aspace: u32,
    table: &mut process::ProcessTable,
    self_endpoint: u32,
)
{
    // SAFETY: ipc_buf is the registered IPC buffer page, page-aligned.
    #[allow(clippy::cast_ptr_alignment)]
    let (cap_count, caps) = unsafe { syscall::read_recv_caps(ipc_buf) };

    if cap_count == 0
    {
        let _ = syscall::ipc_reply(1, 0, &[]); // InvalidElf
        return;
    }

    match process::create_process(caps[0], pool, self_aspace, table, self_endpoint)
    {
        Some(result) => reply_create_result(&result),
        None =>
        {
            let _ = syscall::ipc_reply(2, 0, &[]); // OutOfMemory
        }
    }
}

/// Handle `REQUEST_FRAMES` — allocate and return physical memory frames.
fn handle_request_frames(ipc_buf: *mut u64, pool: &mut FramePool)
{
    // SAFETY: ipc_buf is the registered IPC buffer, kernel wrote data words.
    let requested = unsafe { core::ptr::read_volatile(ipc_buf) };

    if requested == 0 || requested > 4
    {
        let _ = syscall::ipc_reply(7, 0, &[]); // InvalidArgument
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
        let _ = syscall::ipc_reply(6, 0, &[]); // OutOfMemory
    }
    else
    {
        // SAFETY: ipc_buf is writable and page-aligned.
        unsafe { core::ptr::write_volatile(ipc_buf, granted) };
        let _ = syscall::ipc_reply(0, 1, &caps[..granted as usize]);
    }
}

/// Handle `CREATE_FROM_VFS` — create a process from a VFS path.
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
        let _ = syscall::ipc_reply(8, 0, &[]); // NoVfsEndpoint
        return;
    }

    let path_len = ((label >> 16) & 0xFFFF) as usize;
    if path_len == 0 || path_len > ipc::MAX_PATH_LEN
    {
        let _ = syscall::ipc_reply(9, 0, &[]); // FileNotFound
        return;
    }

    let mut path_buf = [0u8; ipc::MAX_PATH_LEN];
    // SAFETY: ipc_buf data words contain path bytes.
    let effective_len = unsafe { ipc::read_path_from_ipc(ipc_buf, path_len, &mut path_buf) };

    match process::create_process_from_vfs(
        vfsd_ep,
        &path_buf[..effective_len],
        pool,
        self_aspace,
        table,
        ipc_buf,
        self_endpoint,
    )
    {
        Ok(result) => reply_create_result(&result),
        Err(code) =>
        {
            let _ = syscall::ipc_reply(code, 0, &[]);
        }
    }
}
