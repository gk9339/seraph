// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// procmgr/src/process.rs

//! Process table, creation, and lifecycle management.
//!
//! Manages the process table and provides functions for creating processes
//! from in-memory ELF images or by streaming from VFS, as well as starting
//! suspended processes.

use crate::frames::{FramePool, PAGE_SIZE};
use crate::loader::{self, TEMP_FRAME_VA, TEMP_MODULE_VA, TEMP_VFS_VA};
use ipc::procmgr_errors;
use process_abi::{
    PROCESS_ABI_VERSION, PROCESS_INFO_VADDR, PROCESS_STACK_PAGES, PROCESS_STACK_TOP,
};

/// IPC buffer VA for child processes.
const CHILD_IPC_BUF_VA: u64 = 0x0000_7FFF_FFFE_0000;

/// Max file data bytes per VFS read IPC. Word 0 = `bytes_read`, words 1..63 = data.
const VFS_CHUNK_SIZE: u64 = 63 * 8; // 504 bytes

/// Next token value (monotonically increasing, never zero).
static NEXT_TOKEN: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(1);

const MAX_PROCESSES: usize = 32;

// ── Process table ───────────────────────────────────────────────────────────

/// Per-process resource record. Fields read when teardown is implemented.
#[allow(dead_code)]
pub struct ProcessEntry
{
    token: u64,
    aspace_cap: u32,
    cspace_cap: u32,
    thread_cap: u32,
    pi_frame_cap: u32,
    entry_point: u64,
    started: bool,
    frames_allocated: u32,
}

pub struct ProcessTable
{
    entries: [Option<ProcessEntry>; MAX_PROCESSES],
}

impl ProcessTable
{
    pub const fn new() -> Self
    {
        const NONE: Option<ProcessEntry> = None;
        Self {
            entries: [NONE; MAX_PROCESSES],
        }
    }

    fn insert(&mut self, entry: ProcessEntry) -> bool
    {
        for slot in &mut self.entries
        {
            if slot.is_none()
            {
                *slot = Some(entry);
                return true;
            }
        }
        false
    }

    fn find_mut_by_token(&mut self, token: u64) -> Option<&mut ProcessEntry>
    {
        self.entries
            .iter_mut()
            .filter_map(|s| s.as_mut())
            .find(|e| e.token == token)
    }
}

// ── Result type ─────────────────────────────────────────────────────────────

/// Result of a successful process creation call.
pub struct CreateResult
{
    /// Tokened endpoint cap for the caller to use with `START_PROCESS`.
    pub process_handle: u32,
    /// Derived Thread cap to transfer to caller (CONTROL right).
    pub thread_for_caller: u32,
}

// ── Child setup helpers ─────────────────────────────────────────────────────

/// Populate a `ProcessInfo` page for a child process and map it read-only.
///
/// Installs the creator endpoint cap (if any) into the child `CSpace` and
/// records its slot in the child's `ProcessInfo`. The cap identifies the
/// creator so the child can issue bootstrap requests on it.
// similar_names: child_aspace/child_cspace are intentionally parallel.
#[allow(clippy::similar_names)]
fn populate_child_info(
    pool: &mut FramePool,
    self_aspace: u32,
    child_aspace: u32,
    child_cspace: u32,
    child_thread: u32,
    creator_endpoint: u32,
) -> Option<u32>
{
    let pi_frame = pool.alloc_page()?;
    syscall::mem_map(
        pi_frame,
        self_aspace,
        TEMP_FRAME_VA,
        0,
        1,
        syscall::MAP_WRITABLE,
    )
    .ok()?;
    // SAFETY: TEMP_FRAME_VA mapped writable, one page.
    unsafe { core::ptr::write_bytes(TEMP_FRAME_VA as *mut u8, 0, PAGE_SIZE as usize) };

    let child_thread_in_child =
        syscall::cap_copy(child_thread, child_cspace, syscall::RIGHTS_THREAD).ok()?;
    let child_aspace_in_child =
        syscall::cap_copy(child_aspace, child_cspace, syscall::RIGHTS_ALL).ok()?;
    let child_cspace_in_child =
        syscall::cap_copy(child_cspace, child_cspace, syscall::RIGHTS_CSPACE).ok()?;

    let creator_ep_in_child = if creator_endpoint != 0
    {
        // Preserve source rights: try ALL first (for services where
        // creator_endpoint doubles as a recv endpoint, e.g. fatfs service ep);
        // fall back to SEND for tokened send caps used by the bootstrap protocol.
        syscall::cap_copy(creator_endpoint, child_cspace, syscall::RIGHTS_ALL)
            .or_else(|_| syscall::cap_copy(creator_endpoint, child_cspace, syscall::RIGHTS_SEND))
            .ok()?
    }
    else
    {
        0
    };

    // SAFETY: TEMP_FRAME_VA is page-aligned and mapped writable.
    let pi = unsafe { process_abi::process_info_mut(TEMP_FRAME_VA) };
    pi.version = PROCESS_ABI_VERSION;
    pi.self_thread_cap = child_thread_in_child;
    pi.self_aspace_cap = child_aspace_in_child;
    pi.self_cspace_cap = child_cspace_in_child;
    pi.ipc_buffer_vaddr = CHILD_IPC_BUF_VA;
    pi.creator_endpoint_cap = creator_ep_in_child;

    let _ = syscall::mem_unmap(self_aspace, TEMP_FRAME_VA, 1);

    let pi_ro = syscall::cap_derive(pi_frame, syscall::RIGHTS_MAP_READ).ok()?;
    syscall::mem_map(pi_ro, child_aspace, PROCESS_INFO_VADDR, 0, 1, 0).ok()?;

    Some(pi_frame)
}

/// Map stack and IPC buffer pages into a child address space.
fn map_child_stack_and_ipc(pool: &mut FramePool, child_aspace: u32) -> Option<()>
{
    let stack_base = PROCESS_STACK_TOP - (PROCESS_STACK_PAGES as u64) * PAGE_SIZE;
    for i in 0..PROCESS_STACK_PAGES
    {
        let frame = pool.alloc_page()?;
        let rw = syscall::cap_derive(frame, syscall::RIGHTS_MAP_RW).ok()?;
        syscall::mem_map(
            rw,
            child_aspace,
            stack_base + (i as u64) * PAGE_SIZE,
            0,
            1,
            0,
        )
        .ok()?;
    }

    let ipc_frame = pool.alloc_page()?;
    let ipc_rw = syscall::cap_derive(ipc_frame, syscall::RIGHTS_MAP_RW).ok()?;
    syscall::mem_map(ipc_rw, child_aspace, CHILD_IPC_BUF_VA, 0, 1, 0).ok()?;

    Some(())
}

/// Determine protection flags for an ELF segment.
fn segment_prot(seg: &elf::LoadSegment) -> u64
{
    if seg.executable
    {
        syscall::MAP_EXECUTABLE
    }
    else if seg.writable
    {
        syscall::MAP_WRITABLE
    }
    else
    {
        syscall::MAP_READONLY
    }
}

/// Derive caller-facing caps and record the process in the table.
// similar_names: child_aspace/child_cspace are intentionally parallel.
// too_many_arguments: grouping these into a struct would add complexity without
// reducing call sites — this helper is called from exactly two places.
#[allow(clippy::similar_names, clippy::too_many_arguments)]
fn finalize_creation(
    pool: &FramePool,
    pages_before: u32,
    child_aspace: u32,
    child_cspace: u32,
    child_thread: u32,
    pi_frame_cap: u32,
    entry_point: u64,
    table: &mut ProcessTable,
    self_endpoint: u32,
) -> Option<CreateResult>
{
    let token = NEXT_TOKEN.fetch_add(1, core::sync::atomic::Ordering::Relaxed);

    // Derive a tokened endpoint cap for the caller. The token identifies this
    // process on subsequent START_PROCESS / REQUEST_FRAMES calls.
    let process_handle =
        syscall::cap_derive_token(self_endpoint, syscall::RIGHTS_SEND_GRANT, token).ok()?;
    let thread_for_caller = syscall::cap_derive(child_thread, syscall::RIGHTS_THREAD).ok()?;

    table.insert(ProcessEntry {
        token,
        aspace_cap: child_aspace,
        cspace_cap: child_cspace,
        thread_cap: child_thread,
        pi_frame_cap,
        entry_point,
        started: false,
        frames_allocated: pool.allocated_pages - pages_before,
    });

    Some(CreateResult {
        process_handle,
        thread_for_caller,
    })
}

// ── Process creation (from memory) ──────────────────────────────────────────

/// Create a process from an in-memory ELF byte slice (suspended).
// similar_names: aspace/cspace are intentionally parallel kernel object names.
#[allow(clippy::similar_names, clippy::too_many_arguments)]
fn create_process_from_bytes(
    module_bytes: &[u8],
    pool: &mut FramePool,
    self_aspace: u32,
    table: &mut ProcessTable,
    self_endpoint: u32,
    creator_endpoint: u32,
) -> Option<CreateResult>
{
    let pages_before = pool.allocated_pages;

    let ehdr = elf::validate(module_bytes, crate::arch::current::EXPECTED_ELF_MACHINE).ok()?;
    let entry = elf::entry_point(ehdr);

    let child_aspace = syscall::cap_create_aspace().ok()?;
    let child_cspace = syscall::cap_create_cspace(256).ok()?;
    let child_thread = syscall::cap_create_thread(child_aspace, child_cspace).ok()?;

    for seg_result in elf::load_segments(ehdr, module_bytes)
    {
        let seg = seg_result.ok()?;
        if seg.memsz == 0
        {
            continue;
        }

        let prot = segment_prot(&seg);
        let first_page = seg.vaddr & !0xFFF;
        let last_page_end = (seg.vaddr + seg.memsz + 0xFFF) & !0xFFF;
        let num_pages = ((last_page_end - first_page) / PAGE_SIZE) as usize;
        let file_data = &module_bytes[seg.offset as usize..(seg.offset + seg.filesz) as usize];

        for page_idx in 0..num_pages
        {
            let page_vaddr = first_page + (page_idx as u64) * PAGE_SIZE;
            loader::load_elf_page(
                page_vaddr,
                seg.vaddr,
                file_data,
                prot,
                pool,
                self_aspace,
                child_aspace,
            )?;
        }
    }

    let pi_frame_cap = populate_child_info(
        pool,
        self_aspace,
        child_aspace,
        child_cspace,
        child_thread,
        creator_endpoint,
    )?;
    map_child_stack_and_ipc(pool, child_aspace)?;

    finalize_creation(
        pool,
        pages_before,
        child_aspace,
        child_cspace,
        child_thread,
        pi_frame_cap,
        entry,
        table,
        self_endpoint,
    )
}

/// Create a process from an ELF module frame cap (suspended).
///
/// Maps the frame, delegates to `create_process_from_bytes`, then unmaps.
pub fn create_process(
    module_frame_cap: u32,
    pool: &mut FramePool,
    self_aspace: u32,
    table: &mut ProcessTable,
    self_endpoint: u32,
    creator_endpoint: u32,
) -> Option<CreateResult>
{
    let module_pages = loader::map_module(module_frame_cap, self_aspace)?;
    let module_size = module_pages * PAGE_SIZE;

    // SAFETY: module frame mapped read-only at TEMP_MODULE_VA for module_size bytes.
    let module_bytes =
        unsafe { core::slice::from_raw_parts(TEMP_MODULE_VA as *const u8, module_size as usize) };

    let result = create_process_from_bytes(
        module_bytes,
        pool,
        self_aspace,
        table,
        self_endpoint,
        creator_endpoint,
    );

    let _ = syscall::mem_unmap(self_aspace, TEMP_MODULE_VA, module_pages);

    result
}

// ── VFS helpers ─────────────────────────────────────────────────────────────

/// Open a file via vfsd namespace resolution. Returns the per-file capability.
fn vfs_open(vfsd_ep: u32, ipc_buf: *mut u64, path: &[u8]) -> Option<u32>
{
    let label = ipc::vfsd_labels::OPEN | ((path.len() as u64) << 16);
    // SAFETY: ipc_buf is the registered IPC buffer page.
    let ipc_ref = unsafe { ipc::IpcBuf::from_raw(ipc_buf) };
    let word_count = ipc::write_path_to_ipc(ipc_ref, path);

    let Ok((reply_label, _)) = syscall::ipc_call(vfsd_ep, label, word_count, &[])
    else
    {
        return None;
    };
    if reply_label != 0
    {
        return None;
    }
    // SAFETY: ipc_buf is the registered IPC buffer.
    #[allow(clippy::cast_ptr_alignment)]
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc_buf) };
    if cap_count == 0
    {
        return None;
    }
    Some(reply_caps[0])
}

/// Read from an open file via its per-file capability.
fn vfs_read(file_cap: u32, ipc_buf: *mut u64, offset: u64, max_len: u64) -> Option<usize>
{
    // SAFETY: ipc_buf is the registered IPC buffer page.
    let ipc = unsafe { ipc::IpcBuf::from_raw(ipc_buf) };
    ipc.write_word(0, offset);
    ipc.write_word(1, max_len);

    let Ok((reply_label, _)) = syscall::ipc_call(file_cap, ipc::fs_labels::FS_READ, 2, &[])
    else
    {
        return None;
    };
    if reply_label != 0
    {
        return None;
    }
    Some(ipc.read_word(0) as usize)
}

/// Stat an open file via its per-file capability.
fn vfs_stat(file_cap: u32, ipc_buf: *mut u64) -> Option<u64>
{
    let Ok((reply_label, _)) = syscall::ipc_call(file_cap, ipc::fs_labels::FS_STAT, 0, &[])
    else
    {
        return None;
    };
    if reply_label != 0
    {
        return None;
    }
    // SAFETY: ipc_buf is the registered IPC buffer page.
    let ipc = unsafe { ipc::IpcBuf::from_raw(ipc_buf) };
    Some(ipc.read_word(0))
}

/// Close an open file via its per-file capability and delete the cap.
fn vfs_close(file_cap: u32, _ipc_buf: *mut u64)
{
    let _ = syscall::ipc_call(file_cap, ipc::fs_labels::FS_CLOSE, 0, &[]);
    let _ = syscall::cap_delete(file_cap);
}

// ── VFS-based ELF loading ──────────────────────────────────────────────────

/// Everything `load_elf_page_streaming` needs beyond the per-page arguments:
/// the VFS file handle, IPC buffer, parent/child address spaces, and the
/// shared frame pool. The borrow on `pool` is mutable for alloc/free; the
/// rest is by value.
pub struct ElfLoadCtx<'a>
{
    pub file_cap: u32,
    pub ipc_buf: *mut u64,
    pub self_aspace: u32,
    pub child_aspace: u32,
    pub pool: &'a mut FramePool,
}

/// Load one ELF segment page by streaming file data from VFS.
fn load_elf_page_streaming(
    page_vaddr: u64,
    seg: &elf::LoadSegment,
    prot: u64,
    ctx: &mut ElfLoadCtx,
) -> Option<()>
{
    let frame_cap = ctx.pool.alloc_page()?;

    syscall::mem_map(
        frame_cap,
        ctx.self_aspace,
        TEMP_FRAME_VA,
        0,
        1,
        syscall::MAP_WRITABLE,
    )
    .ok()?;
    // SAFETY: TEMP_FRAME_VA mapped writable, one page.
    unsafe { core::ptr::write_bytes(TEMP_FRAME_VA as *mut u8, 0, PAGE_SIZE as usize) };

    stream_segment_to_frame(page_vaddr, seg, ctx.file_cap, ctx.ipc_buf);

    let _ = syscall::mem_unmap(ctx.self_aspace, TEMP_FRAME_VA, 1);

    let derived = loader::derive_frame_for_prot(frame_cap, prot)?;
    syscall::mem_map(derived, ctx.child_aspace, page_vaddr, 0, 1, 0).ok()?;

    Some(())
}

/// Stream segment file data from VFS into the frame mapped at `TEMP_FRAME_VA`.
fn stream_segment_to_frame(
    page_vaddr: u64,
    seg: &elf::LoadSegment,
    file_cap: u32,
    ipc_buf: *mut u64,
)
{
    let copy_start_va = page_vaddr.max(seg.vaddr);
    let copy_end_va = (page_vaddr + PAGE_SIZE).min(seg.vaddr + seg.filesz);

    if copy_start_va >= copy_end_va
    {
        return;
    }

    let dest_offset = (copy_start_va - page_vaddr) as usize;
    let file_offset = seg.offset + (copy_start_va - seg.vaddr);
    let bytes_to_read = copy_end_va - copy_start_va;

    let mut read_pos = 0u64;
    while read_pos < bytes_to_read
    {
        let chunk = VFS_CHUNK_SIZE.min(bytes_to_read - read_pos);
        let Some(bytes_read) = vfs_read(file_cap, ipc_buf, file_offset + read_pos, chunk)
        else
        {
            break;
        };
        if bytes_read == 0
        {
            break;
        }
        let safe_len = (bytes_read as u64).min(bytes_to_read - read_pos) as usize;

        // SAFETY: ipc_buf data[1..] contains file data; TEMP_FRAME_VA is
        // mapped writable; dest_offset + read_pos + safe_len <= PAGE_SIZE.
        unsafe {
            core::ptr::copy_nonoverlapping(
                ipc_buf.add(1) as *const u8,
                (TEMP_FRAME_VA as *mut u8).add(dest_offset + read_pos as usize),
                safe_len,
            );
        }
        read_pos += safe_len as u64;
    }
}

/// Create a process by streaming an ELF binary from the VFS.
///
/// Reads only the ELF header page, then loads each segment page-by-page
/// directly from vfsd into target frames. No intermediate file buffer.
// clippy::too_many_lines: VFS-based process creation is one transaction that
// owns the lifetime of the ELF-header scratch frame, the child's kernel
// objects, and the per-page streaming loop. Splitting scatters the
// error/cleanup paths that must unwind in a fixed order against a single
// `FramePool` borrow, and introduces helpers that each need the full context
// struct anyway. The sub-phases are already factored through named helpers
// (vfs_open/stat/read/close, load_elf_page_streaming, populate_child_info,
// map_child_stack_and_ipc, finalize_creation); what remains is the linear
// orchestration.
#[allow(clippy::similar_names, clippy::too_many_lines)]
pub fn create_process_from_vfs(
    ctx: &crate::ProcmgrCtx,
    path: &[u8],
    pool: &mut FramePool,
    table: &mut ProcessTable,
    ipc_buf: *mut u64,
    creator_endpoint: u32,
) -> Result<CreateResult, u64>
{
    let vfsd_ep = ctx.vfsd_ep;
    let self_aspace = ctx.self_aspace;
    let self_endpoint = ctx.self_endpoint;
    let file_cap = vfs_open(vfsd_ep, ipc_buf, path).ok_or(procmgr_errors::FILE_NOT_FOUND)?;
    let file_size = vfs_stat(file_cap, ipc_buf).ok_or(procmgr_errors::IO_ERROR)?;

    if file_size == 0
    {
        vfs_close(file_cap, ipc_buf);
        return Err(procmgr_errors::INVALID_ELF);
    }

    // Allocate one frame for the ELF header page.
    let hdr_frame = pool.alloc_page().ok_or_else(|| {
        vfs_close(file_cap, ipc_buf);
        procmgr_errors::OUT_OF_MEMORY
    })?;
    syscall::mem_map(
        hdr_frame,
        self_aspace,
        TEMP_VFS_VA,
        0,
        1,
        syscall::MAP_WRITABLE,
    )
    .map_err(|_| {
        vfs_close(file_cap, ipc_buf);
        procmgr_errors::OUT_OF_MEMORY
    })?;
    // SAFETY: TEMP_VFS_VA mapped writable, one page.
    unsafe { core::ptr::write_bytes(TEMP_VFS_VA as *mut u8, 0, PAGE_SIZE as usize) };

    // Read the first page (ELF header + program headers).
    let hdr_size = file_size.min(PAGE_SIZE);
    let mut offset: u64 = 0;
    while offset < hdr_size
    {
        let chunk = VFS_CHUNK_SIZE.min(hdr_size - offset);
        let bytes_read =
            vfs_read(file_cap, ipc_buf, offset, chunk).ok_or(procmgr_errors::IO_ERROR)?;
        if bytes_read == 0
        {
            break;
        }
        let safe_len = bytes_read.min(VFS_CHUNK_SIZE as usize);
        // SAFETY: ipc_buf data[1..] contains file data; TEMP_VFS_VA mapped writable.
        unsafe {
            core::ptr::copy_nonoverlapping(
                ipc_buf.add(1) as *const u8,
                (TEMP_VFS_VA as *mut u8).add(offset as usize),
                safe_len,
            );
        }
        offset += safe_len as u64;
    }

    // Parse ELF headers from the header page.
    // SAFETY: TEMP_VFS_VA is mapped and contains `offset` bytes of file data.
    let header_data =
        unsafe { core::slice::from_raw_parts(TEMP_VFS_VA as *const u8, offset as usize) };
    let ehdr = elf::validate(header_data, crate::arch::current::EXPECTED_ELF_MACHINE)
        .map_err(|_| procmgr_errors::INVALID_ELF)?;
    let entry = elf::entry_point(ehdr);

    let pages_before = pool.allocated_pages;

    let child_aspace = syscall::cap_create_aspace().map_err(|_| procmgr_errors::OUT_OF_MEMORY)?;
    let child_cspace =
        syscall::cap_create_cspace(256).map_err(|_| procmgr_errors::OUT_OF_MEMORY)?;
    let child_thread = syscall::cap_create_thread(child_aspace, child_cspace)
        .map_err(|_| procmgr_errors::OUT_OF_MEMORY)?;

    // Stream each LOAD segment page-by-page from VFS.
    for seg_result in elf::load_segments_metadata(ehdr, header_data, file_size)
    {
        let seg = seg_result.map_err(|_| procmgr_errors::INVALID_ELF)?;
        if seg.memsz == 0
        {
            continue;
        }

        let prot = segment_prot(&seg);
        let first_page = seg.vaddr & !0xFFF;
        let last_page_end = (seg.vaddr + seg.memsz + 0xFFF) & !0xFFF;
        let num_pages = ((last_page_end - first_page) / PAGE_SIZE) as usize;

        for page_idx in 0..num_pages
        {
            let page_vaddr = first_page + (page_idx as u64) * PAGE_SIZE;
            let mut load_ctx = ElfLoadCtx {
                file_cap,
                ipc_buf,
                self_aspace,
                child_aspace,
                pool,
            };
            load_elf_page_streaming(page_vaddr, &seg, prot, &mut load_ctx)
                .ok_or(procmgr_errors::INVALID_ELF)?;
        }
    }

    // Done reading — unmap header page and close file.
    let _ = syscall::mem_unmap(self_aspace, TEMP_VFS_VA, 1);
    pool.free_page(hdr_frame);
    vfs_close(file_cap, ipc_buf);

    let pi_frame_cap = populate_child_info(
        pool,
        self_aspace,
        child_aspace,
        child_cspace,
        child_thread,
        creator_endpoint,
    )
    .ok_or(procmgr_errors::OUT_OF_MEMORY)?;
    map_child_stack_and_ipc(pool, child_aspace).ok_or(procmgr_errors::OUT_OF_MEMORY)?;

    finalize_creation(
        pool,
        pages_before,
        child_aspace,
        child_cspace,
        child_thread,
        pi_frame_cap,
        entry,
        table,
        self_endpoint,
    )
    .ok_or(procmgr_errors::OUT_OF_MEMORY)
}

// ── Process start ───────────────────────────────────────────────────────────

/// Start a previously created (suspended) process.
///
/// Calls `thread_configure` and `thread_start` on the process's thread.
pub fn start_process(token: u64, table: &mut ProcessTable) -> Result<(), u64>
{
    let entry = table
        .find_mut_by_token(token)
        .ok_or(procmgr_errors::INVALID_TOKEN)?;

    if entry.started
    {
        return Err(procmgr_errors::ALREADY_STARTED);
    }

    syscall::thread_configure(
        entry.thread_cap,
        entry.entry_point,
        PROCESS_STACK_TOP,
        PROCESS_INFO_VADDR,
    )
    .map_err(|_| 3u64)?;

    syscall::thread_start(entry.thread_cap).map_err(|_| 3u64)?;

    entry.started = true;
    Ok(())
}
