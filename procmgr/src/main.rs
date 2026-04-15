// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// procmgr/src/main.rs

//! Seraph process manager — IPC server for process lifecycle management.
//!
//! Receives `CREATE_PROCESS` requests via IPC, loads ELF images, creates
//! address spaces, and prepares new processes in a suspended state. The
//! caller injects capabilities and patches `ProcessInfo`, then calls
//! `START_PROCESS` to begin execution.
//!
//! Also supports VFS-based loading via `CREATE_PROCESS_FROM_VFS` once a
//! vfsd endpoint has been configured via `SET_VFSD_ENDPOINT`.
//!
//! See `procmgr/docs/ipc-interface.md`.

#![no_std]
#![no_main]
// cast_possible_truncation: targets 64-bit only; u64↔usize conversions lossless.
#![allow(clippy::cast_possible_truncation)]

extern crate runtime;

use process_abi::{
    ProcessInfo, StartupInfo, PROCESS_ABI_VERSION, PROCESS_INFO_VADDR, PROCESS_STACK_PAGES,
    PROCESS_STACK_TOP,
};

// ── Constants ────────────────────────────────────────────────────────────────

const PAGE_SIZE: u64 = 0x1000;

/// IPC label for `CREATE_PROCESS`.
const LABEL_CREATE_PROCESS: u64 = 1;

/// IPC label for `START_PROCESS`.
const LABEL_START_PROCESS: u64 = 2;

/// IPC label for `REQUEST_FRAMES`.
const LABEL_REQUEST_FRAMES: u64 = 5;

/// IPC label for `CREATE_PROCESS_FROM_VFS`.
const LABEL_CREATE_PROCESS_FROM_VFS: u64 = 6;

/// IPC label for `SET_VFSD_ENDPOINT`.
const LABEL_SET_VFSD_ENDPOINT: u64 = 7;

/// Temp VA base for mapping module frames during ELF parsing.
const TEMP_MODULE_VA: u64 = 0x0000_0000_8000_0000; // 2 GiB

/// Temp VA for writing into freshly allocated frames.
const TEMP_FRAME_VA: u64 = 0x0000_0000_9000_0000; // 2.25 GiB

/// Temp VA for mapping VFS buffer frames during ELF loading.
const TEMP_VFS_VA: u64 = 0x0000_0000_A000_0000; // 2.5 GiB

/// IPC buffer VA for child processes.
const CHILD_IPC_BUF_VA: u64 = 0x0000_7FFF_FFFE_0000;

/// Next process ID (monotonically increasing).
static NEXT_PID: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(1);

/// Max file data bytes per VFS read IPC. Word 0 = `bytes_read`, words 1..63 = data.
const VFS_CHUNK_SIZE: u64 = 63 * 8; // 504 bytes

// ── Architecture constants ──────────────────────────────────────────────────

mod arch
{
    #[cfg(target_arch = "x86_64")]
    pub const EXPECTED_ELF_MACHINE: u16 = elf::EM_X86_64;

    #[cfg(target_arch = "riscv64")]
    pub const EXPECTED_ELF_MACHINE: u16 = elf::EM_RISCV;
}

// ── Frame allocator with free list ──────────────────────────────────────────

/// Maximum number of freed pages that can be tracked.
const MAX_FREE_PAGES: usize = 64;

/// Frame allocator over initial caps delegated by init, with a free list.
///
/// init copies memory frame caps into procmgr's `CSpace` at
/// `initial_caps_base..+initial_caps_count`. This allocator splits pages from
/// those frames using `frame_split`. Freed pages go to a free list and are
/// reused before allocating fresh pages.
struct FramePool
{
    current_cap: u32,
    next_idx: u32,
    base: u32,
    count: u32,
    allocated_pages: u32,
    free_list: [u32; MAX_FREE_PAGES],
    free_count: usize,
}

impl FramePool
{
    fn new(base: u32, count: u32) -> Self
    {
        Self {
            current_cap: 0,
            next_idx: 0,
            base,
            count,
            allocated_pages: 0,
            free_list: [0; MAX_FREE_PAGES],
            free_count: 0,
        }
    }

    fn alloc_page(&mut self) -> Option<u32>
    {
        // Check free list first.
        if self.free_count > 0
        {
            self.free_count -= 1;
            let cap = self.free_list[self.free_count];
            self.allocated_pages += 1;
            return Some(cap);
        }

        loop
        {
            // Try splitting a page from the current frame.
            if self.current_cap != 0
            {
                if let Ok((page, rest)) = syscall::frame_split(self.current_cap, PAGE_SIZE)
                {
                    self.current_cap = rest;
                    self.allocated_pages += 1;
                    return Some(page);
                }
                // Split failed — current frame is one page or less. Use it directly.
                let cap = self.current_cap;
                self.current_cap = 0;
                self.allocated_pages += 1;
                return Some(cap);
            }

            // Advance to next frame from init's delegation.
            if self.next_idx >= self.count
            {
                return None;
            }
            self.current_cap = self.base + self.next_idx;
            self.next_idx += 1;
        }
    }

    /// Return a single-page frame cap to the free list for reuse.
    fn free_page(&mut self, cap: u32)
    {
        if self.free_count < MAX_FREE_PAGES
        {
            self.free_list[self.free_count] = cap;
            self.free_count += 1;
            if self.allocated_pages > 0
            {
                self.allocated_pages -= 1;
            }
        }
        // If free list is full, the cap is leaked. Acceptable for now.
    }
}

// ── Process table ───────────────────────────────────────────────────────────

/// Per-process resource record. Fields read when teardown is implemented.
#[allow(dead_code)]
struct ProcessEntry
{
    pid: u64,
    aspace_cap: u32,
    cspace_cap: u32,
    thread_cap: u32,
    /// Frame cap for the `ProcessInfo` page (retained for lifecycle management).
    pi_frame_cap: u32,
    /// ELF entry point virtual address (stored at create, used at start).
    entry_point: u64,
    /// Whether the process thread has been started.
    started: bool,
    frames_allocated: u32,
}

const MAX_PROCESSES: usize = 32;

struct ProcessTable
{
    entries: [Option<ProcessEntry>; MAX_PROCESSES],
}

impl ProcessTable
{
    const fn new() -> Self
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

    fn find_mut(&mut self, pid: u64) -> Option<&mut ProcessEntry>
    {
        self.entries
            .iter_mut()
            .filter_map(|s| s.as_mut())
            .find(|e| e.pid == pid)
    }
}

// ── Process creation ─────────────────────────────────────────────────────────

/// Map a module frame read-only, probing for the exact mappable page count.
///
/// Starts from a high estimate and decrements by one page until the mapping
/// succeeds. Returns the number of pages successfully mapped, or `None` if
/// unmappable.
fn map_module(module_frame_cap: u32, self_aspace: u32) -> Option<u64>
{
    // Try from 128 pages (512 KiB) down to 1 page, decrementing by 1.
    // This is slower than binary search but guarantees we find the exact
    // frame size, which is critical for correct ELF slice bounds.
    let mut module_pages: u64 = 128;
    while module_pages > 0
    {
        if syscall::mem_map(
            module_frame_cap,
            self_aspace,
            TEMP_MODULE_VA,
            0,
            module_pages,
            syscall::PROT_READ,
        )
        .is_ok()
        {
            return Some(module_pages);
        }
        module_pages -= 1;
    }
    None
}

/// Derive a frame cap with the given protection rights for mapping.
fn derive_frame_for_prot(frame_cap: u32, prot: u64) -> Option<u32>
{
    if prot == syscall::PROT_EXEC
    {
        syscall::cap_derive(frame_cap, 0x1 | 0x4).ok() // MAP | EXECUTE
    }
    else if prot == syscall::PROT_WRITE
    {
        syscall::cap_derive(frame_cap, 0x1 | 0x2).ok() // MAP | WRITE
    }
    else
    {
        syscall::cap_derive(frame_cap, 0x1).ok() // MAP only
    }
}

/// Copy one ELF segment page into a fresh frame and map it into the child.
fn load_elf_page(
    page_vaddr: u64,
    seg_vaddr: u64,
    file_data: &[u8],
    prot: u64,
    pool: &mut FramePool,
    self_aspace: u32,
    child_aspace: u32,
) -> Option<()>
{
    let frame_cap = pool.alloc_page()?;

    syscall::mem_map(
        frame_cap,
        self_aspace,
        TEMP_FRAME_VA,
        0,
        1,
        syscall::PROT_WRITE,
    )
    .ok()?;
    // SAFETY: TEMP_FRAME_VA mapped writable, one page.
    unsafe { core::ptr::write_bytes(TEMP_FRAME_VA as *mut u8, 0, PAGE_SIZE as usize) };

    let page_start_in_seg = page_vaddr.saturating_sub(seg_vaddr) as usize;
    let page_end_in_seg = page_start_in_seg + PAGE_SIZE as usize;
    let file_start = page_start_in_seg.min(file_data.len());
    let file_end = page_end_in_seg.min(file_data.len());
    if file_start < file_end
    {
        let dest_offset = if page_vaddr < seg_vaddr
        {
            (seg_vaddr - page_vaddr) as usize
        }
        else
        {
            0
        };
        let avail = PAGE_SIZE as usize - dest_offset;
        let copy_len = (file_end - file_start).min(avail);
        let src = &file_data[file_start..file_start + copy_len];
        // SAFETY: TEMP_FRAME_VA mapped writable; copy stays within one page
        // because copy_len <= PAGE_SIZE - dest_offset.
        unsafe {
            core::ptr::copy_nonoverlapping(
                src.as_ptr(),
                (TEMP_FRAME_VA as *mut u8).add(dest_offset),
                src.len(),
            );
        }
    }

    let _ = syscall::mem_unmap(self_aspace, TEMP_FRAME_VA, 1);

    let derived = derive_frame_for_prot(frame_cap, prot)?;
    syscall::mem_map(derived, child_aspace, page_vaddr, 0, 1, 0).ok()?;

    Some(())
}

/// Load one ELF segment page by streaming file data from VFS.
///
/// Allocates a frame, reads the relevant file bytes from vfsd in 504-byte
/// chunks directly into the frame, then maps it into the child address space.
#[allow(clippy::too_many_arguments)]
fn load_elf_page_streaming(
    page_vaddr: u64,
    seg: &elf::LoadSegment,
    vfsd_ep: u32,
    fd: u64,
    ipc_buf: *mut u64,
    prot: u64,
    pool: &mut FramePool,
    self_aspace: u32,
    child_aspace: u32,
) -> Option<()>
{
    let frame_cap = pool.alloc_page()?;

    syscall::mem_map(
        frame_cap,
        self_aspace,
        TEMP_FRAME_VA,
        0,
        1,
        syscall::PROT_WRITE,
    )
    .ok()?;
    // SAFETY: TEMP_FRAME_VA mapped writable, one page.
    unsafe { core::ptr::write_bytes(TEMP_FRAME_VA as *mut u8, 0, PAGE_SIZE as usize) };

    // Compute which file bytes land on this page.
    let copy_start_va = page_vaddr.max(seg.vaddr);
    let copy_end_va = (page_vaddr + PAGE_SIZE).min(seg.vaddr + seg.filesz);

    if copy_start_va < copy_end_va
    {
        let dest_offset = (copy_start_va - page_vaddr) as usize;
        let file_offset = seg.offset + (copy_start_va - seg.vaddr);
        let bytes_to_read = copy_end_va - copy_start_va;

        let mut read_pos = 0u64;
        while read_pos < bytes_to_read
        {
            let chunk = VFS_CHUNK_SIZE.min(bytes_to_read - read_pos);
            let bytes_read = vfs_read(vfsd_ep, ipc_buf, fd, file_offset + read_pos, chunk)?;
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

    let _ = syscall::mem_unmap(self_aspace, TEMP_FRAME_VA, 1);

    let derived = derive_frame_for_prot(frame_cap, prot)?;
    syscall::mem_map(derived, child_aspace, page_vaddr, 0, 1, 0).ok()?;

    Some(())
}

/// Populate a `ProcessInfo` page for a child process and map it read-only.
///
/// Returns the frame cap for the `ProcessInfo` page (retained for patching
/// during `START_PROCESS`).
// similar_names: child_aspace/child_cspace are intentionally parallel.
#[allow(clippy::similar_names)]
fn populate_child_info(
    pool: &mut FramePool,
    self_aspace: u32,
    child_aspace: u32,
    child_cspace: u32,
    child_thread: u32,
) -> Option<u32>
{
    let pi_frame = pool.alloc_page()?;
    syscall::mem_map(
        pi_frame,
        self_aspace,
        TEMP_FRAME_VA,
        0,
        1,
        syscall::PROT_WRITE,
    )
    .ok()?;
    // SAFETY: TEMP_FRAME_VA mapped writable, one page.
    unsafe { core::ptr::write_bytes(TEMP_FRAME_VA as *mut u8, 0, PAGE_SIZE as usize) };

    let child_thread_in_child = syscall::cap_copy(child_thread, child_cspace, !0u64).ok()?;
    let child_aspace_in_child = syscall::cap_copy(child_aspace, child_cspace, !0u64).ok()?;
    let child_cspace_in_child = syscall::cap_copy(child_cspace, child_cspace, !0u64).ok()?;

    // SAFETY: TEMP_FRAME_VA is page-aligned and mapped writable.
    #[allow(clippy::cast_ptr_alignment)]
    let pi = unsafe { &mut *(TEMP_FRAME_VA as *mut ProcessInfo) };
    pi.version = PROCESS_ABI_VERSION;
    pi.self_thread_cap = child_thread_in_child;
    pi.self_aspace_cap = child_aspace_in_child;
    pi.self_cspace_cap = child_cspace_in_child;
    pi.ipc_buffer_vaddr = CHILD_IPC_BUF_VA;
    pi.parent_endpoint_cap = 0;
    pi.initial_caps_base = 0;
    pi.initial_caps_count = 0;
    pi.cap_descriptor_count = 0;
    pi.cap_descriptors_offset = core::mem::size_of::<ProcessInfo>() as u32;
    pi.startup_message_offset = 0;
    pi.startup_message_len = 0;
    pi._pad = 0;

    let _ = syscall::mem_unmap(self_aspace, TEMP_FRAME_VA, 1);

    let pi_ro = syscall::cap_derive(pi_frame, 0x1).ok()?; // MAP only
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
        let rw = syscall::cap_derive(frame, 0x1 | 0x2).ok()?; // MAP | WRITE
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
    let ipc_rw = syscall::cap_derive(ipc_frame, 0x1 | 0x2).ok()?; // MAP | WRITE
    syscall::mem_map(ipc_rw, child_aspace, CHILD_IPC_BUF_VA, 0, 1, 0).ok()?;

    Some(())
}

/// Result of a successful process creation call.
struct CreateResult
{
    pid: u64,
    /// Derived `CSpace` cap to transfer to caller (full rights).
    cspace_for_caller: u32,
    /// Derived `ProcessInfo` frame cap to transfer to caller (MAP|WRITE).
    pi_frame_for_caller: u32,
    /// Derived Thread cap to transfer to caller (CONTROL right).
    thread_for_caller: u32,
}

/// Create a process from an in-memory ELF byte slice.
///
/// The process is created in a **suspended** state — the thread is not
/// started. The caller must inject capabilities and call `START_PROCESS`.
// similar_names: aspace/cspace are intentionally parallel kernel object names.
#[allow(clippy::similar_names)]
fn create_process_from_bytes(
    module_bytes: &[u8],
    pool: &mut FramePool,
    self_aspace: u32,
    table: &mut ProcessTable,
) -> Option<CreateResult>
{
    let pages_before = pool.allocated_pages;

    let ehdr = elf::validate(module_bytes, arch::EXPECTED_ELF_MACHINE).ok()?;
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

        let prot = if seg.executable
        {
            syscall::PROT_EXEC
        }
        else if seg.writable
        {
            syscall::PROT_WRITE
        }
        else
        {
            syscall::PROT_READ
        };

        let first_page = seg.vaddr & !0xFFF;
        let last_page_end = (seg.vaddr + seg.memsz + 0xFFF) & !0xFFF;
        let num_pages = ((last_page_end - first_page) / PAGE_SIZE) as usize;
        let file_data = &module_bytes[seg.offset as usize..(seg.offset + seg.filesz) as usize];

        for page_idx in 0..num_pages
        {
            let page_vaddr = first_page + (page_idx as u64) * PAGE_SIZE;
            load_elf_page(
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

    let pi_frame_cap =
        populate_child_info(pool, self_aspace, child_aspace, child_cspace, child_thread)?;
    map_child_stack_and_ipc(pool, child_aspace)?;

    // Derive caps to transfer to caller. procmgr retains originals.
    let cspace_for_caller = syscall::cap_derive(child_cspace, !0u64).ok()?;
    let pi_frame_for_caller = syscall::cap_derive(pi_frame_cap, 0x1 | 0x2).ok()?; // MAP | WRITE
                                                                                  // CONTROL right so caller can bind death notifications.
    let thread_for_caller = syscall::cap_derive(child_thread, !0u64).ok()?;

    let pid = NEXT_PID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);

    table.insert(ProcessEntry {
        pid,
        aspace_cap: child_aspace,
        cspace_cap: child_cspace,
        thread_cap: child_thread,
        pi_frame_cap,
        entry_point: entry,
        started: false,
        frames_allocated: pool.allocated_pages - pages_before,
    });

    Some(CreateResult {
        pid,
        cspace_for_caller,
        pi_frame_for_caller,
        thread_for_caller,
    })
}

/// Create a process from an ELF module frame cap.
///
/// Maps the frame, delegates to `create_process_from_bytes`, then unmaps.
fn create_process(
    module_frame_cap: u32,
    pool: &mut FramePool,
    self_aspace: u32,
    table: &mut ProcessTable,
) -> Option<CreateResult>
{
    let module_pages = map_module(module_frame_cap, self_aspace)?;
    let module_size = module_pages * PAGE_SIZE;

    // SAFETY: module frame mapped read-only at TEMP_MODULE_VA for module_size bytes.
    let module_bytes =
        unsafe { core::slice::from_raw_parts(TEMP_MODULE_VA as *const u8, module_size as usize) };

    let result = create_process_from_bytes(module_bytes, pool, self_aspace, table);

    let _ = syscall::mem_unmap(self_aspace, TEMP_MODULE_VA, module_pages);

    result
}

// ── VFS-based ELF loading ──────────────────────────────────────────────────

/// Open a file via vfsd. Returns the file descriptor on success.
fn vfs_open(vfsd_ep: u32, ipc_buf: *mut u64, path: &[u8]) -> Option<u64>
{
    let label = 1u64 | ((path.len() as u64) << 16);
    // Pack path bytes into IPC buffer data words.
    let word_count = path.len().div_ceil(8);
    for i in 0..word_count
    {
        let mut word = 0u64;
        for j in 0..8
        {
            let idx = i * 8 + j;
            if idx < path.len()
            {
                word |= u64::from(path[idx]) << (j * 8);
            }
        }
        // SAFETY: ipc_buf is the registered IPC buffer page.
        unsafe { core::ptr::write_volatile(ipc_buf.add(i), word) };
    }

    let Ok((reply_label, _)) = syscall::ipc_call(vfsd_ep, label, word_count, &[])
    else
    {
        return None;
    };

    if reply_label != 0
    {
        return None;
    }

    // SAFETY: ipc_buf contains reply data.
    let fd = unsafe { core::ptr::read_volatile(ipc_buf) };
    Some(fd)
}

/// Read up to 512 bytes from a file via vfsd. Returns bytes read.
fn vfs_read(vfsd_ep: u32, ipc_buf: *mut u64, fd: u64, offset: u64, max_len: u64) -> Option<usize>
{
    // SAFETY: ipc_buf is the registered IPC buffer page.
    unsafe {
        core::ptr::write_volatile(ipc_buf, fd);
        core::ptr::write_volatile(ipc_buf.add(1), offset);
        core::ptr::write_volatile(ipc_buf.add(2), max_len);
    }

    let Ok((reply_label, _)) = syscall::ipc_call(vfsd_ep, 2, 3, &[])
    else
    {
        return None;
    };

    if reply_label != 0
    {
        return None;
    }

    // SAFETY: ipc_buf contains reply data[0] = bytes_read.
    let bytes_read = unsafe { core::ptr::read_volatile(ipc_buf) } as usize;
    Some(bytes_read)
}

/// Get file size via vfsd STAT. Returns file size.
fn vfs_stat(vfsd_ep: u32, ipc_buf: *mut u64, fd: u64) -> Option<u64>
{
    // SAFETY: ipc_buf is the registered IPC buffer page.
    unsafe { core::ptr::write_volatile(ipc_buf, fd) };

    let Ok((reply_label, _)) = syscall::ipc_call(vfsd_ep, 4, 1, &[])
    else
    {
        return None;
    };

    if reply_label != 0
    {
        return None;
    }

    // SAFETY: ipc_buf contains reply data[0] = file_size.
    Some(unsafe { core::ptr::read_volatile(ipc_buf) })
}

/// Close a file via vfsd.
fn vfs_close(vfsd_ep: u32, ipc_buf: *mut u64, fd: u64)
{
    // SAFETY: ipc_buf is the registered IPC buffer page.
    unsafe { core::ptr::write_volatile(ipc_buf, fd) };
    let _ = syscall::ipc_call(vfsd_ep, 3, 1, &[]);
}

/// Create a process by streaming an ELF binary from the VFS.
///
/// Reads only the ELF header page, then loads each segment page-by-page
/// directly from vfsd into target frames. No intermediate file buffer.
#[allow(clippy::similar_names, clippy::too_many_lines)]
fn create_process_from_vfs(
    vfsd_ep: u32,
    path: &[u8],
    pool: &mut FramePool,
    self_aspace: u32,
    table: &mut ProcessTable,
    ipc_buf: *mut u64,
) -> Result<CreateResult, u64>
{
    let fd = vfs_open(vfsd_ep, ipc_buf, path).ok_or(9u64)?; // FileNotFound
    let file_size = vfs_stat(vfsd_ep, ipc_buf, fd).ok_or(10u64)?; // IoError

    if file_size == 0
    {
        vfs_close(vfsd_ep, ipc_buf, fd);
        return Err(1); // InvalidElf
    }

    // Allocate one frame for the ELF header page.
    let hdr_frame = pool.alloc_page().ok_or_else(|| {
        vfs_close(vfsd_ep, ipc_buf, fd);
        2u64
    })?;
    syscall::mem_map(
        hdr_frame,
        self_aspace,
        TEMP_VFS_VA,
        0,
        1,
        syscall::PROT_WRITE,
    )
    .map_err(|_| {
        vfs_close(vfsd_ep, ipc_buf, fd);
        2u64
    })?;
    // SAFETY: TEMP_VFS_VA mapped writable, one page.
    unsafe { core::ptr::write_bytes(TEMP_VFS_VA as *mut u8, 0, PAGE_SIZE as usize) };

    // Read the first page (ELF header + program headers).
    let hdr_size = file_size.min(PAGE_SIZE);
    let mut offset: u64 = 0;
    while offset < hdr_size
    {
        let chunk = VFS_CHUNK_SIZE.min(hdr_size - offset);
        let bytes_read = vfs_read(vfsd_ep, ipc_buf, fd, offset, chunk).ok_or(10u64)?;
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
    let ehdr = elf::validate(header_data, arch::EXPECTED_ELF_MACHINE).map_err(|_| 1u64)?;
    let entry = elf::entry_point(ehdr);

    let pages_before = pool.allocated_pages;

    let child_aspace = syscall::cap_create_aspace().map_err(|_| 2u64)?;
    let child_cspace = syscall::cap_create_cspace(256).map_err(|_| 2u64)?;
    let child_thread = syscall::cap_create_thread(child_aspace, child_cspace).map_err(|_| 2u64)?;

    // Stream each LOAD segment page-by-page from VFS.
    for seg_result in elf::load_segments_metadata(ehdr, header_data, file_size)
    {
        let seg = seg_result.map_err(|_| 1u64)?;
        if seg.memsz == 0
        {
            continue;
        }

        let prot = if seg.executable
        {
            syscall::PROT_EXEC
        }
        else if seg.writable
        {
            syscall::PROT_WRITE
        }
        else
        {
            syscall::PROT_READ
        };

        let first_page = seg.vaddr & !0xFFF;
        let last_page_end = (seg.vaddr + seg.memsz + 0xFFF) & !0xFFF;
        let num_pages = ((last_page_end - first_page) / PAGE_SIZE) as usize;

        for page_idx in 0..num_pages
        {
            let page_vaddr = first_page + (page_idx as u64) * PAGE_SIZE;
            load_elf_page_streaming(
                page_vaddr,
                &seg,
                vfsd_ep,
                fd,
                ipc_buf,
                prot,
                pool,
                self_aspace,
                child_aspace,
            )
            .ok_or(1u64)?;
        }
    }

    // Done reading — unmap header page and close file.
    let _ = syscall::mem_unmap(self_aspace, TEMP_VFS_VA, 1);
    pool.free_page(hdr_frame);
    vfs_close(vfsd_ep, ipc_buf, fd);

    let pi_frame_cap =
        populate_child_info(pool, self_aspace, child_aspace, child_cspace, child_thread)
            .ok_or(1u64)?;
    map_child_stack_and_ipc(pool, child_aspace).ok_or(1u64)?;

    let cspace_for_caller = syscall::cap_derive(child_cspace, !0u64).ok().ok_or(1u64)?;
    let pi_frame_for_caller = syscall::cap_derive(pi_frame_cap, 0x1 | 0x2)
        .ok()
        .ok_or(1u64)?; // MAP | WRITE
    let thread_for_caller = syscall::cap_derive(child_thread, !0u64).ok().ok_or(1u64)?;

    let pid = NEXT_PID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);

    table.insert(ProcessEntry {
        pid,
        aspace_cap: child_aspace,
        cspace_cap: child_cspace,
        thread_cap: child_thread,
        pi_frame_cap,
        entry_point: entry,
        started: false,
        frames_allocated: pool.allocated_pages - pages_before,
    });

    Ok(CreateResult {
        pid,
        cspace_for_caller,
        pi_frame_for_caller,
        thread_for_caller,
    })
}

// ── Process start ───────────────────────────────────────────────────────────

/// Start a previously created (suspended) process.
///
/// Calls `thread_configure` and `thread_start` on the process's thread.
fn start_process(pid: u64, table: &mut ProcessTable) -> Result<(), u64>
{
    let entry = table.find_mut(pid).ok_or(4u64)?; // InvalidPid

    if entry.started
    {
        return Err(5); // AlreadyStarted
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

// ── Entry point ──────────────────────────────────────────────────────────────

// too_many_lines: IPC dispatch loop with inline request handling; splitting each
// label into a separate function would obscure the sequential flow.
#[allow(clippy::too_many_lines)]
#[no_mangle]
extern "Rust" fn main(startup: &StartupInfo) -> !
{
    // Register IPC buffer.
    if syscall::ipc_buffer_set(startup.ipc_buffer as u64).is_err()
    {
        syscall::thread_exit();
    }

    let endpoint = startup.parent_endpoint;
    let self_aspace = startup.self_aspace;
    // cast_ptr_alignment: IPC buffer is page-aligned (4096), exceeding u64 alignment.
    #[allow(clippy::cast_ptr_alignment)]
    let ipc_buf = startup.ipc_buffer.cast::<u64>();

    // Read initial cap slot range from the ProcessInfo page directly.
    // SAFETY: PROCESS_INFO_VADDR is mapped read-only by procmgr's creator (init).
    // cast_ptr_alignment: PROCESS_INFO_VADDR is page-aligned.
    #[allow(clippy::cast_ptr_alignment)]
    let proc_info = unsafe { &*(PROCESS_INFO_VADDR as *const ProcessInfo) };
    let mut pool = FramePool::new(proc_info.initial_caps_base, proc_info.initial_caps_count);
    let mut table = ProcessTable::new();
    let mut vfsd_ep: u32 = 0;

    // IPC receive loop.
    loop
    {
        let Ok((label, _count)) = syscall::ipc_recv(endpoint)
        else
        {
            continue;
        };

        let opcode = label & 0xFFFF;

        match opcode
        {
            LABEL_CREATE_PROCESS =>
            {
                // Read transferred cap from IPC buffer.
                // SAFETY: ipc_buf is the registered IPC buffer page, page-aligned.
                #[allow(clippy::cast_ptr_alignment)]
                let (cap_count, caps) = unsafe { syscall::read_recv_caps(ipc_buf) };

                if cap_count == 0
                {
                    let _ = syscall::ipc_reply(1, 0, &[]); // InvalidElf
                    continue;
                }

                let module_frame_cap = caps[0];

                match create_process(module_frame_cap, &mut pool, self_aspace, &mut table)
                {
                    Some(result) =>
                    {
                        // Write pid to IPC buffer data[0] for the reply.
                        // SAFETY: ipc_buf is writable and page-aligned.
                        unsafe {
                            core::ptr::write_volatile(ipc_buf, result.pid);
                        }
                        let _ = syscall::ipc_reply(
                            0,
                            1,
                            &[
                                result.cspace_for_caller,
                                result.pi_frame_for_caller,
                                result.thread_for_caller,
                            ],
                        );
                    }
                    None =>
                    {
                        let _ = syscall::ipc_reply(2, 0, &[]); // OutOfMemory
                    }
                }
            }

            LABEL_START_PROCESS =>
            {
                // Read pid from IPC buffer data[0].
                // SAFETY: ipc_buf is the registered IPC buffer, kernel wrote data words.
                let pid = unsafe { core::ptr::read_volatile(ipc_buf) };

                match start_process(pid, &mut table)
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

            LABEL_REQUEST_FRAMES =>
            {
                // Read requested page count from IPC buffer data[0].
                // SAFETY: ipc_buf is the registered IPC buffer, kernel wrote data words.
                let requested = unsafe { core::ptr::read_volatile(ipc_buf) };

                if requested == 0 || requested > 4
                {
                    let _ = syscall::ipc_reply(7, 0, &[]); // InvalidArgument
                    continue;
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
                    // Write granted count to IPC buffer data[0].
                    // SAFETY: ipc_buf is writable and page-aligned.
                    unsafe { core::ptr::write_volatile(ipc_buf, granted) };
                    let _ = syscall::ipc_reply(0, 1, &caps[..granted as usize]);
                }
            }

            LABEL_CREATE_PROCESS_FROM_VFS =>
            {
                if vfsd_ep == 0
                {
                    let _ = syscall::ipc_reply(8, 0, &[]); // NoVfsEndpoint
                    continue;
                }

                let path_len = ((label >> 16) & 0xFFFF) as usize;
                if path_len == 0 || path_len > 48
                {
                    let _ = syscall::ipc_reply(9, 0, &[]); // FileNotFound
                    continue;
                }

                // Extract path from IPC buffer.
                let mut path_buf = [0u8; 48];
                let word_count = path_len.div_ceil(8);
                for i in 0..word_count
                {
                    // SAFETY: ipc_buf data words contain path bytes.
                    let word = unsafe { core::ptr::read_volatile(ipc_buf.add(i)) };
                    for j in 0..8
                    {
                        let idx = i * 8 + j;
                        if idx < path_len
                        {
                            path_buf[idx] = (word >> (j * 8)) as u8;
                        }
                    }
                }

                let path = &path_buf[..path_len];

                match create_process_from_vfs(
                    vfsd_ep,
                    path,
                    &mut pool,
                    self_aspace,
                    &mut table,
                    ipc_buf,
                )
                {
                    Ok(result) =>
                    {
                        // SAFETY: ipc_buf is writable and page-aligned.
                        unsafe {
                            core::ptr::write_volatile(ipc_buf, result.pid);
                        }
                        let _ = syscall::ipc_reply(
                            0,
                            1,
                            &[
                                result.cspace_for_caller,
                                result.pi_frame_for_caller,
                                result.thread_for_caller,
                            ],
                        );
                    }
                    Err(code) =>
                    {
                        let _ = syscall::ipc_reply(code, 0, &[]);
                    }
                }
            }

            LABEL_SET_VFSD_ENDPOINT =>
            {
                // Read transferred cap from IPC buffer.
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
