// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// procmgr/src/main.rs

//! Seraph process manager — IPC server for process lifecycle management.
//!
//! Receives `CREATE_PROCESS` requests via IPC, loads ELF images, creates
//! address spaces, and prepares new processes in a suspended state. The
//! caller injects capabilities and patches `ProcessInfo`, then calls
//! `START_PROCESS` to begin execution. See `procmgr/docs/ipc-interface.md`.

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

/// Temp VA base for mapping module frames during ELF parsing.
const TEMP_MODULE_VA: u64 = 0x0000_0000_8000_0000; // 2 GiB

/// Temp VA for writing into freshly allocated frames.
const TEMP_FRAME_VA: u64 = 0x0000_0000_9000_0000; // 2.25 GiB

/// IPC buffer VA for child processes.
const CHILD_IPC_BUF_VA: u64 = 0x0000_7FFF_FFFE_0000;

/// Next process ID (monotonically increasing).
static NEXT_PID: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(1);

// ── Architecture constants ──────────────────────────────────────────────────

mod arch
{
    #[cfg(target_arch = "x86_64")]
    pub const EXPECTED_ELF_MACHINE: u16 = elf::EM_X86_64;

    #[cfg(target_arch = "riscv64")]
    pub const EXPECTED_ELF_MACHINE: u16 = elf::EM_RISCV;
}

// ── Simple frame allocator ───────────────────────────────────────────────────

/// Frame allocator over initial caps delegated by init.
///
/// init copies memory frame caps into procmgr's `CSpace` at
/// `initial_caps_base..+initial_caps_count`. This allocator splits pages from
/// those frames using `frame_split`.
struct FramePool
{
    current_cap: u32,
    next_idx: u32,
    base: u32,
    count: u32,
    allocated_pages: u32,
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
        }
    }

    fn alloc_page(&mut self) -> Option<u32>
    {
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

/// Result of a successful `create_process` call.
struct CreateResult
{
    pid: u64,
    /// Derived `CSpace` cap to transfer to caller (full rights).
    cspace_for_caller: u32,
    /// Derived `ProcessInfo` frame cap to transfer to caller (MAP\|WRITE).
    pi_frame_for_caller: u32,
}

/// Create a process from an ELF module frame cap received via IPC.
///
/// The process is created in a **suspended** state — the thread is not
/// started. The caller must inject capabilities and call `START_PROCESS`.
///
/// Returns the PID and derived caps for the caller on success.
// similar_names: aspace/cspace are intentionally parallel kernel object names.
#[allow(clippy::similar_names)]
fn create_process(
    module_frame_cap: u32,
    pool: &mut FramePool,
    self_aspace: u32,
    table: &mut ProcessTable,
) -> Option<CreateResult>
{
    let pages_before = pool.allocated_pages;

    let module_pages = map_module(module_frame_cap, self_aspace)?;
    let module_size = module_pages * PAGE_SIZE;

    // SAFETY: module frame mapped read-only at TEMP_MODULE_VA for module_size bytes.
    let module_bytes =
        unsafe { core::slice::from_raw_parts(TEMP_MODULE_VA as *const u8, module_size as usize) };

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

    let _ = syscall::mem_unmap(self_aspace, TEMP_MODULE_VA, module_pages);

    let pi_frame_cap =
        populate_child_info(pool, self_aspace, child_aspace, child_cspace, child_thread)?;
    map_child_stack_and_ipc(pool, child_aspace)?;

    // Derive caps to transfer to caller. Procmgr retains originals.
    let cspace_for_caller = syscall::cap_derive(child_cspace, !0u64).ok()?;
    let pi_frame_for_caller = syscall::cap_derive(pi_frame_cap, 0x1 | 0x2).ok()?; // MAP | WRITE

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
    })
}

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

    // IPC receive loop.
    loop
    {
        let Ok((label, _count)) = syscall::ipc_recv(endpoint)
        else
        {
            continue;
        };

        match label
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
                            &[result.cspace_for_caller, result.pi_frame_for_caller],
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

            _ =>
            {
                let _ = syscall::ipc_reply(0xFFFF, 0, &[]);
            }
        }
    }
}
