// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// init/src/main.rs

//! Seraph init — bootstrap service.
//!
//! First userspace process. Reads `InitInfo` from the kernel, starts procmgr
//! directly via raw syscalls, then requests procmgr to create devmgr via IPC.
//! Exits after bootstrap is complete.

#![no_std]
#![no_main]
// cast_possible_truncation: init targets 64-bit only; u64↔usize conversions
// are lossless. u32 casts on capability slot indices and struct offsets are
// bounded by CSpace capacity and page size.
#![allow(clippy::cast_possible_truncation)]

use core::panic::PanicInfo;

use init_protocol::{CapDescriptor, CapType, InitInfo, INIT_PROTOCOL_VERSION};
use process_abi::{
    ProcessInfo, PROCESS_ABI_VERSION, PROCESS_INFO_VADDR, PROCESS_STACK_PAGES, PROCESS_STACK_TOP,
};

// ── Constants ────────────────────────────────────────────────────────────────

/// Page size (4 KiB).
const PAGE_SIZE: u64 = 0x1000;

/// Base virtual address for temporary mappings in init's address space.
/// Well above init's code/data/stack to avoid conflicts.
const TEMP_MAP_BASE: u64 = 0x0000_0001_0000_0000; // 4 GiB

/// Virtual address for init's own IPC buffer page (explicitly mapped).
const INIT_IPC_BUF_VA: u64 = 0x0000_0000_C000_0000; // 3 GiB

/// Virtual address for the IPC buffer in procmgr's address space.
const PROCMGR_IPC_BUF_VA: u64 = 0x0000_7FFF_FFFE_0000;

/// Virtual address for the log thread's IPC buffer (separate from main thread).
const LOG_THREAD_IPC_BUF_VA: u64 = 0x0000_0000_C000_1000; // main IPC buf + 1 page

/// Virtual address for the log thread's stack base.
const LOG_THREAD_STACK_VA: u64 = 0x0000_0000_D000_0000;

/// Number of stack pages for the log thread (16 KiB).
const LOG_THREAD_STACK_PAGES: u64 = 4;

/// IPC label for `CREATE_PROCESS` (per `procmgr/docs/ipc-interface.md`).
const LABEL_CREATE_PROCESS: u64 = 1;

/// IPC label for `START_PROCESS` (per `procmgr/docs/ipc-interface.md`).
const LABEL_START_PROCESS: u64 = 2;

/// IPC label for `CREATE_PROCESS_FROM_VFS` (per `procmgr/docs/ipc-interface.md`).
const LABEL_CREATE_FROM_VFS: u64 = 6;

/// IPC label for `SET_VFSD_ENDPOINT` (per `procmgr/docs/ipc-interface.md`).
const LABEL_SET_VFSD_EP: u64 = 7;

/// IPC label for `REGISTER_SERVICE` (per `svcmgr/docs/ipc-interface.md`).
const LABEL_REGISTER_SERVICE: u64 = 1;

/// IPC label for `HANDOVER_COMPLETE` (per `svcmgr/docs/ipc-interface.md`).
const LABEL_HANDOVER_COMPLETE: u64 = 2;

/// Sentinel value in `CapDescriptor.aux0` indicating a procmgr endpoint.
const PROCMGR_ENDPOINT_SENTINEL: u64 = 0xFFFF_FFFF_FFFF_FFFB;

// (IPC buffer is explicitly mapped at INIT_IPC_BUF_VA using a fresh frame.)

// ── Architecture-specific code (serial output, ELF machine type) ─────────────

mod arch;

/// Log endpoint cap slot for IPC-based logging (set after log thread starts).
static mut LOG_EP_SLOT: u32 = 0;

/// IPC buffer pointer for the main thread (set after IPC buffer is mapped).
static mut MAIN_IPC_BUF: *mut u64 = core::ptr::null_mut();

/// Log a message. Uses direct serial before the log thread is running,
/// then switches to IPC-based logging through the log thread.
fn log(s: &str)
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

/// Direct serial output (early boot, before log thread exists).
fn serial_log(s: &str)
{
    for &b in s.as_bytes()
    {
        if b == b'\n'
        {
            arch::serial_write_byte(b'\r');
        }
        arch::serial_write_byte(b);
    }
    arch::serial_write_byte(b'\r');
    arch::serial_write_byte(b'\n');
}

/// IPC-based logging through the log thread.
///
/// Matches the protocol in `runtime::log`: label = `LOG_LABEL_BASE` | (len << 16),
/// data words = packed bytes, up to 48 bytes per chunk.
fn ipc_log(log_ep: u32, ipc_buf: *mut u64, s: &str)
{
    let bytes = s.as_bytes();
    let total_len = bytes.len();
    let chunk_size = 6 * 8; // 48 bytes per chunk (6 data words)
    let mut offset = 0;

    while offset < total_len || total_len == 0
    {
        let remaining = total_len - offset;
        let chunk_len = remaining.min(chunk_size);
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
            // SAFETY: IPC buffer is valid.
            unsafe { core::ptr::write_volatile(ipc_buf.add(i), word) };
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

// ── Cap descriptor helpers ───────────────────────────────────────────────────

fn descriptors(info: &InitInfo) -> &[CapDescriptor]
{
    let base = core::ptr::from_ref::<InitInfo>(info).cast::<u8>();
    // SAFETY: InitInfo page is valid; cap_descriptors_offset and count are
    // populated by the kernel in Phase 9.
    #[allow(clippy::cast_ptr_alignment)]
    unsafe {
        let ptr = base
            .add(info.cap_descriptors_offset as usize)
            .cast::<CapDescriptor>();
        core::slice::from_raw_parts(ptr, info.cap_descriptor_count as usize)
    }
}

// dead_code: used by the x86_64 serial module but not riscv64.
#[allow(dead_code)]
fn find_cap_by_type(info: &InitInfo, wanted: CapType) -> Option<u32>
{
    descriptors(info)
        .iter()
        .find(|d| d.cap_type == wanted)
        .map(|d| d.slot)
}

// dead_code: used by the riscv64 serial module but not x86_64.
#[allow(dead_code)]
fn find_cap(info: &InitInfo, wanted_type: CapType, wanted_aux0: u64) -> Option<u32>
{
    descriptors(info)
        .iter()
        .find(|d| d.cap_type == wanted_type && d.aux0 == wanted_aux0)
        .map(|d| d.slot)
}

// ── Architecture constants ──────────────────────────────────────────────────

// ── Simple frame allocator ───────────────────────────────────────────────────

/// Bump allocator over init's memory pool frame caps.
///
/// Splits page-sized frames from the first available memory pool frame cap
/// using `frame_split`. When a frame is exhausted, moves to the next.
struct FrameAlloc
{
    /// Current frame cap being split (covers remaining unallocated region).
    current: u32,
    /// Remaining size in bytes of current frame.
    remaining: u64,
    /// Index into the memory frame range for the next frame to use.
    next_idx: u32,
    /// [`InitInfo`] fields copied out for reference.
    frame_base: u32,
    frame_count: u32,
}

impl FrameAlloc
{
    fn new(info: &InitInfo) -> Self
    {
        Self {
            current: 0,
            remaining: 0,
            next_idx: 0,
            frame_base: info.memory_frame_base,
            frame_count: info.memory_frame_count,
        }
    }

    /// Allocate a single 4 KiB page frame. Returns the Frame cap slot index.
    fn alloc_page(&mut self) -> Option<u32>
    {
        // If current frame is exhausted or not yet set, advance to next.
        while self.remaining < PAGE_SIZE
        {
            if self.next_idx >= self.frame_count
            {
                return None; // Out of memory
            }
            self.current = self.frame_base + self.next_idx;
            self.next_idx += 1;

            // Look up the frame size from the cap descriptor.
            // For simplicity, try splitting; if the frame is exactly one page,
            // frame_split will fail and we use it directly.
            // We don't know the frame size without querying, so we try a split
            // and handle the error.
            self.remaining = u64::MAX; // Will be refined on first split
        }

        if self.remaining == PAGE_SIZE
        {
            // Exactly one page left — use the cap directly.
            self.remaining = 0;
            Some(self.current)
        }
        else
        {
            // Split off one page from the front.
            if let Ok((page_cap, rest_cap)) = syscall::frame_split(self.current, PAGE_SIZE)
            {
                self.current = rest_cap;
                if self.remaining != u64::MAX
                {
                    self.remaining -= PAGE_SIZE;
                }
                Some(page_cap)
            }
            else
            {
                // Split failed — frame may be exactly one page or smaller.
                // Use it directly and mark as exhausted.
                self.remaining = 0;
                Some(self.current)
            }
        }
    }

    /// Allocate a page, map it writable at `va` in `aspace`, zero it, and
    /// return the frame cap.
    fn alloc_zero_page(&mut self, aspace: u32, va: u64) -> Option<u32>
    {
        let cap = self.alloc_page()?;
        syscall::mem_map(cap, aspace, va, 0, 1, syscall::PROT_WRITE).ok()?;
        // SAFETY: va is mapped writable and covers one page.
        unsafe {
            core::ptr::write_bytes(va as *mut u8, 0, PAGE_SIZE as usize);
        }
        Some(cap)
    }
}

// ── ELF loading ──────────────────────────────────────────────────────────────

/// Derive a frame cap with the given protection rights for mapping.
fn derive_frame_for_prot(frame_cap: u32, prot: u64) -> Option<u32>
{
    if prot == syscall::PROT_READ
    {
        syscall::cap_derive(frame_cap, 0x1).ok() // MAP only
    }
    else if prot == syscall::PROT_EXEC
    {
        syscall::cap_derive(frame_cap, 0x1 | 0x4).ok() // MAP | EXECUTE
    }
    else
    {
        syscall::cap_derive(frame_cap, 0x1 | 0x2).ok() // MAP | WRITE
    }
}

/// Scratch VA for per-page frame writes during ELF loading.
const ELF_PAGE_TEMP_VA: u64 = TEMP_MAP_BASE + 0x1000_0000;

/// Copy one ELF segment page from `file_data` into a freshly allocated frame,
/// then map it into the target address space.
fn load_elf_page(
    page_vaddr: u64,
    seg_vaddr: u64,
    file_data: &[u8],
    prot: u64,
    alloc: &mut FrameAlloc,
    init_aspace: u32,
    target_aspace: u32,
) -> Option<()>
{
    let Some(frame_cap) = alloc.alloc_page()
    else
    {
        log("init: ELF load: frame alloc failed");
        return None;
    };

    if syscall::mem_map(
        frame_cap,
        init_aspace,
        ELF_PAGE_TEMP_VA,
        0,
        1,
        syscall::PROT_WRITE,
    )
    .is_err()
    {
        log("init: ELF load: temp map failed");
        return None;
    }

    // SAFETY: ELF_PAGE_TEMP_VA is mapped writable, covers one page.
    unsafe { core::ptr::write_bytes(ELF_PAGE_TEMP_VA as *mut u8, 0, PAGE_SIZE as usize) };

    let dest_offset = if page_vaddr < seg_vaddr
    {
        (seg_vaddr - page_vaddr) as usize
    }
    else
    {
        0
    };
    let seg_offset = page_vaddr.saturating_sub(seg_vaddr) as usize;
    let avail_in_page = PAGE_SIZE as usize - dest_offset;
    let copy_len = avail_in_page.min(file_data.len().saturating_sub(seg_offset));
    if copy_len > 0
    {
        let src = &file_data[seg_offset..seg_offset + copy_len];
        // SAFETY: temp_va is mapped and writable; copy within one page.
        unsafe {
            core::ptr::copy_nonoverlapping(
                src.as_ptr(),
                (ELF_PAGE_TEMP_VA as *mut u8).add(dest_offset),
                src.len(),
            );
        }
    }

    let _ = syscall::mem_unmap(init_aspace, ELF_PAGE_TEMP_VA, 1);

    let derived_cap = derive_frame_for_prot(frame_cap, prot)?;
    syscall::mem_map(derived_cap, target_aspace, page_vaddr, 0, 1, 0).ok()?;

    Some(())
}

/// Load an ELF image into a target address space.
///
/// `module_bytes` is the raw ELF data (already mapped into init's address space).
/// `target_aspace` is the cap for the target address space.
/// `alloc` provides fresh frames.
/// `init_aspace` is init's own address space cap (for temp mappings).
///
/// Returns the entry point virtual address.
fn load_elf(
    module_bytes: &[u8],
    target_aspace: u32,
    alloc: &mut FrameAlloc,
    init_aspace: u32,
) -> Option<u64>
{
    let ehdr = elf::validate(module_bytes, arch::EXPECTED_ELF_MACHINE).ok()?;
    let entry = elf::entry_point(ehdr);

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
                alloc,
                init_aspace,
                target_aspace,
            )?;
        }
    }

    Some(entry)
}

// ── Bootstrap procmgr ────────────────────────────────────────────────────────

/// Populate procmgr's `ProcessInfo` page, map it read-only into procmgr.
///
/// Returns the frame cap for the `ProcessInfo` page.
// similar_names: pm_aspace/pm_cspace, pm_aspace_in_pm/pm_cspace_in_pm are
// intentionally parallel names matching the kernel object types.
#[allow(clippy::similar_names)]
/// Procmgr kernel object caps needed to populate `ProcessInfo`.
struct ProcmgrCaps
{
    aspace: u32,
    cspace: u32,
    thread: u32,
    endpoint_slot: u32,
    initial_caps_base: u32,
    initial_caps_count: u32,
}

// similar_names: pm_aspace_in_pm / pm_cspace_in_pm are intentionally parallel.
#[allow(clippy::similar_names)]
fn populate_procmgr_info(
    alloc: &mut FrameAlloc,
    init_aspace: u32,
    caps: &ProcmgrCaps,
) -> Option<u32>
{
    let pi_frame = alloc.alloc_zero_page(init_aspace, TEMP_MAP_BASE)?;

    // SAFETY: TEMP_MAP_BASE is mapped writable and zeroed, one page.
    #[allow(clippy::cast_ptr_alignment)]
    let pi = unsafe { &mut *(TEMP_MAP_BASE as *mut ProcessInfo) };

    let pm_thread_in_pm = syscall::cap_copy(caps.thread, caps.cspace, !0u64).ok()?;
    let pm_aspace_in_pm = syscall::cap_copy(caps.aspace, caps.cspace, !0u64).ok()?;
    let pm_cspace_in_pm = syscall::cap_copy(caps.cspace, caps.cspace, !0u64).ok()?;

    pi.version = PROCESS_ABI_VERSION;
    pi.self_thread_cap = pm_thread_in_pm;
    pi.self_aspace_cap = pm_aspace_in_pm;
    pi.self_cspace_cap = pm_cspace_in_pm;
    pi.ipc_buffer_vaddr = PROCMGR_IPC_BUF_VA;
    pi.parent_endpoint_cap = caps.endpoint_slot;
    pi.initial_caps_base = caps.initial_caps_base;
    pi.initial_caps_count = caps.initial_caps_count;
    pi.cap_descriptor_count = 0;
    pi.cap_descriptors_offset = core::mem::size_of::<ProcessInfo>() as u32;
    pi.startup_message_offset = 0;
    pi.startup_message_len = 0;
    pi._pad = 0;

    let _ = syscall::mem_unmap(init_aspace, TEMP_MAP_BASE, 1);

    let pi_ro_cap = syscall::cap_derive(pi_frame, 0x1).ok()?; // MAP only
    syscall::mem_map(pi_ro_cap, caps.aspace, PROCESS_INFO_VADDR, 0, 1, 0).ok()?;

    Some(pi_frame)
}

/// Map stack and IPC buffer pages into the target address space.
fn map_stack_and_ipc(alloc: &mut FrameAlloc, target_aspace: u32, ipc_buf_va: u64) -> Option<()>
{
    let stack_base = PROCESS_STACK_TOP - (PROCESS_STACK_PAGES as u64) * PAGE_SIZE;
    for i in 0..PROCESS_STACK_PAGES
    {
        let frame = alloc.alloc_page()?;
        let rw_cap = syscall::cap_derive(frame, 0x1 | 0x2).ok()?; // MAP | WRITE
        syscall::mem_map(
            rw_cap,
            target_aspace,
            stack_base + (i as u64) * PAGE_SIZE,
            0,
            1,
            0,
        )
        .ok()?;
    }

    let ipc_frame = alloc.alloc_page()?;
    let ipc_rw_cap = syscall::cap_derive(ipc_frame, 0x1 | 0x2).ok()?; // MAP | WRITE
    syscall::mem_map(ipc_rw_cap, target_aspace, ipc_buf_va, 0, 1, 0).ok()?;

    Some(())
}

/// Create and start procmgr from its boot module ELF image.
///
/// Returns the endpoint cap for sending IPC to procmgr.
// similar_names: pm_aspace/pm_cspace are intentionally parallel kernel object names.
#[allow(clippy::similar_names)]
fn bootstrap_procmgr(info: &InitInfo, alloc: &mut FrameAlloc) -> Option<u32>
{
    let init_aspace = info.aspace_cap;

    let module_frame_cap = info.module_frame_base; // Module 0 = procmgr

    let module_size = descriptors(info)
        .iter()
        .find(|d| d.slot == module_frame_cap)
        .map(|d| d.aux1)?;

    let module_pages = (module_size + 0xFFF) / PAGE_SIZE;

    syscall::mem_map(
        module_frame_cap,
        init_aspace,
        TEMP_MAP_BASE,
        0,
        module_pages,
        syscall::PROT_READ,
    )
    .ok()?;

    // SAFETY: module frame is now mapped read-only at TEMP_MAP_BASE.
    let module_bytes =
        unsafe { core::slice::from_raw_parts(TEMP_MAP_BASE as *const u8, module_size as usize) };

    let pm_aspace = syscall::cap_create_aspace().ok()?;
    let pm_cspace = syscall::cap_create_cspace(8192).ok()?;
    let pm_thread = syscall::cap_create_thread(pm_aspace, pm_cspace).ok()?;

    log("init: created procmgr kernel objects");
    log("init: loading procmgr ELF segments");

    let entry = load_elf(module_bytes, pm_aspace, alloc, init_aspace)?;
    let _ = syscall::mem_unmap(init_aspace, TEMP_MAP_BASE, module_pages);

    log("init: loaded procmgr ELF");

    let endpoint_cap = syscall::cap_create_endpoint().ok()?;
    let pm_ep_slot = syscall::cap_copy(endpoint_cap, pm_cspace, !0u64).ok()?;

    // Delegate all remaining memory frame caps to procmgr (derive-twice
    // pattern: derive intermediary in init's CSpace, copy intermediary into
    // procmgr's CSpace). Init retains root + intermediary for revocation.
    let pm_initial_caps_base = pm_ep_slot + 1;
    let mut pm_initial_count: u32 = 0;
    let frames_to_give = info.memory_frame_count.saturating_sub(alloc.next_idx);
    for i in 0..frames_to_give
    {
        let src_slot = info.memory_frame_base + alloc.next_idx + i;
        if let Ok(intermediary) = syscall::cap_derive(src_slot, !0u64)
        {
            if syscall::cap_copy(intermediary, pm_cspace, !0u64).is_ok()
            {
                pm_initial_count += 1;
            }
        }
    }
    alloc.next_idx += frames_to_give;

    let pm_caps = ProcmgrCaps {
        aspace: pm_aspace,
        cspace: pm_cspace,
        thread: pm_thread,
        endpoint_slot: pm_ep_slot,
        initial_caps_base: pm_initial_caps_base,
        initial_caps_count: pm_initial_count,
    };
    populate_procmgr_info(alloc, init_aspace, &pm_caps)?;

    map_stack_and_ipc(alloc, pm_aspace, PROCMGR_IPC_BUF_VA)?;

    syscall::thread_configure(pm_thread, entry, PROCESS_STACK_TOP, PROCESS_INFO_VADDR).ok()?;
    syscall::thread_start(pm_thread).ok()?;

    log("init: procmgr started");

    Some(endpoint_cap)
}

// ── Hardware cap delegation to devmgr ────────────────────────────────────────

/// Temp VA for mapping child `ProcessInfo` frames during cap descriptor patching.
const CHILD_PI_TEMP_VA: u64 = TEMP_MAP_BASE + 0x2000_0000;

/// Max cap descriptors for devmgr delegation.
const DEVMGR_MAX_DESCS: usize = 128;

/// Max cap descriptors for vfsd delegation.
const VFSD_MAX_DESCS: usize = 16;

/// Sentinel value in `CapDescriptor.aux0` indicating a log endpoint.
const LOG_ENDPOINT_SENTINEL: u64 = 0xFFFF_FFFF_FFFF_FFFF;

/// Sentinel value in `CapDescriptor.aux0` indicating a service endpoint.
const SERVICE_ENDPOINT_SENTINEL: u64 = 0xFFFF_FFFF_FFFF_FFFE;

/// Sentinel value in `CapDescriptor.aux0` indicating a registry endpoint.
const REGISTRY_ENDPOINT_SENTINEL: u64 = 0xFFFF_FFFF_FFFF_FFFD;

/// Derive-twice a capability and record a [`CapDescriptor`] for it.
///
/// Derives an intermediary in init's `CSpace` (retained for revocation), copies
/// into the child `CSpace`. Appends a descriptor to `desc_buf`.
#[allow(clippy::too_many_arguments)]
fn inject_cap_desc(
    src_slot: u32,
    cap_type: CapType,
    aux0: u64,
    aux1: u64,
    child_cspace: u32,
    desc_buf: &mut [CapDescriptor],
    desc_count: &mut usize,
    first_slot: &mut u32,
)
{
    let Ok(intermediary) = syscall::cap_derive(src_slot, !0u64)
    else
    {
        return;
    };
    let Ok(child_slot) = syscall::cap_copy(intermediary, child_cspace, !0u64)
    else
    {
        return;
    };
    if *desc_count == 0
    {
        *first_slot = child_slot;
    }
    if *desc_count < desc_buf.len()
    {
        desc_buf[*desc_count] = CapDescriptor {
            slot: child_slot,
            cap_type,
            pad: [0; 3],
            aux0,
            aux1,
        };
        *desc_count += 1;
    }
}

/// Create devmgr with full hardware cap delegation.
///
/// Two-phase process creation: `CREATE_PROCESS` (suspended), inject hardware
/// caps and driver module caps, patch `ProcessInfo` with `CapDescriptor`
/// entries, then `START_PROCESS`.
// too_many_lines: hardware cap delegation is inherently sequential with many
// cap operations; splitting would fragment the delegation flow.
#[allow(clippy::too_many_lines)]
fn create_devmgr_with_caps(
    info: &InitInfo,
    procmgr_ep: u32,
    log_ep: u32,
    registry_ep: u32,
    ipc_buf: *mut u64,
)
{
    let devmgr_frame_cap = info.module_frame_base + 1; // Module 1 = devmgr

    // Phase 1: CREATE_PROCESS (suspended).
    let Ok((reply_label, _)) =
        syscall::ipc_call(procmgr_ep, LABEL_CREATE_PROCESS, 0, &[devmgr_frame_cap])
    else
    {
        log("init: devmgr: CREATE_PROCESS ipc_call failed");
        return;
    };
    if reply_label != 0
    {
        log("init: devmgr: CREATE_PROCESS failed");
        return;
    }

    // Read pid from IPC buffer data[0].
    // SAFETY: IPC buffer is valid and kernel wrote reply data.
    let pid = unsafe { core::ptr::read_volatile(ipc_buf) };

    // Read child CSpace cap and ProcessInfo frame cap from reply caps.
    // SAFETY: ipc_buf is the registered IPC buffer.
    #[allow(clippy::cast_ptr_alignment)]
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc_buf) };
    if cap_count < 2
    {
        log("init: devmgr: CREATE_PROCESS reply missing caps");
        return;
    }
    let child_cspace = reply_caps[0];
    let pi_frame = reply_caps[1];

    // Phase 2: Inject hardware caps.
    // Derive-twice pattern: derive intermediary in init's CSpace, copy into
    // child's CSpace. Init retains intermediary for revocation authority.

    // We track cap descriptors to write into ProcessInfo later.
    let mut desc_buf: [CapDescriptor; DEVMGR_MAX_DESCS] = [CapDescriptor {
        slot: 0,
        cap_type: CapType::Frame,
        pad: [0; 3],
        aux0: 0,
        aux1: 0,
    }; DEVMGR_MAX_DESCS];
    let mut desc_count: usize = 0;
    let mut first_slot: u32 = 0;

    // Inject all hardware caps from init's cap descriptor table.
    let init_descs = descriptors(info);
    for d in init_descs
    {
        match d.cap_type
        {
            CapType::MmioRegion
            | CapType::PciEcam
            | CapType::Interrupt
            | CapType::IoPortRange
            | CapType::SchedControl =>
            {
                inject_cap_desc(
                    d.slot,
                    d.cap_type,
                    d.aux0,
                    d.aux1,
                    child_cspace,
                    &mut desc_buf,
                    &mut desc_count,
                    &mut first_slot,
                );
            }
            // Skip memory frames and SBI control — devmgr doesn't need them.
            _ =>
            {}
        }
    }

    // Inject procmgr endpoint cap so devmgr can spawn drivers and request frames.
    // Use Frame cap type with aux0=0 as a sentinel — devmgr identifies the
    // procmgr endpoint by its position (first non-hardware cap after hw caps).
    inject_cap_desc(
        procmgr_ep,
        CapType::Frame,
        0, // sentinel: endpoint marker
        0,
        child_cspace,
        &mut desc_buf,
        &mut desc_count,
        &mut first_slot,
    );

    // Inject driver module frame cap (module 3 = virtio-blk only; module 4+
    // are filesystem drivers delegated to vfsd instead).
    if info.module_frame_count > 3
    {
        let module_cap = info.module_frame_base + 3;
        inject_cap_desc(
            module_cap,
            CapType::Frame,
            3, // aux0 = module index
            0,
            child_cspace,
            &mut desc_buf,
            &mut desc_count,
            &mut first_slot,
        );
    }

    // Inject log endpoint cap (sentinel: Frame with aux0=LOG_ENDPOINT_SENTINEL).
    inject_cap_desc(
        log_ep,
        CapType::Frame,
        LOG_ENDPOINT_SENTINEL,
        0,
        child_cspace,
        &mut desc_buf,
        &mut desc_count,
        &mut first_slot,
    );

    // Inject device registry endpoint (sentinel: REGISTRY_ENDPOINT_SENTINEL).
    inject_cap_desc(
        registry_ep,
        CapType::Frame,
        REGISTRY_ENDPOINT_SENTINEL,
        0,
        child_cspace,
        &mut desc_buf,
        &mut desc_count,
        &mut first_slot,
    );

    // Phase 3: Patch ProcessInfo page with CapDescriptors.
    // Map the PI frame writable into init's address space.
    if syscall::mem_map(
        pi_frame,
        info.aspace_cap,
        CHILD_PI_TEMP_VA,
        0,
        1,
        syscall::PROT_WRITE,
    )
    .is_err()
    {
        log("init: devmgr: cannot map ProcessInfo frame");
        return;
    }

    // Write initial_caps_base, initial_caps_count, cap_descriptors.
    // SAFETY: CHILD_PI_TEMP_VA is mapped writable to the ProcessInfo page.
    #[allow(clippy::cast_ptr_alignment)]
    let pi = unsafe { &mut *(CHILD_PI_TEMP_VA as *mut ProcessInfo) };

    pi.initial_caps_base = first_slot;
    pi.initial_caps_count = desc_count as u32;
    pi.cap_descriptor_count = desc_count as u32;

    // CapDescriptors start after the ProcessInfo header.
    let descs_offset = core::mem::size_of::<ProcessInfo>() as u32;
    // Align to 8 bytes (CapDescriptor contains u64 fields).
    let descs_offset_aligned = (descs_offset + 7) & !7;
    pi.cap_descriptors_offset = descs_offset_aligned;

    // Write CapDescriptor entries.
    let desc_size = core::mem::size_of::<CapDescriptor>();
    for (i, desc) in desc_buf.iter().enumerate().take(desc_count)
    {
        let byte_offset = descs_offset_aligned as usize + i * desc_size;
        if byte_offset + desc_size > PAGE_SIZE as usize
        {
            break; // Page overflow — shouldn't happen with <128 descriptors.
        }
        // SAFETY: byte_offset is within the mapped page; descs_offset_aligned is
        // 8-byte aligned and CapDescriptor is 24 bytes (multiple of 8), so the
        // destination pointer satisfies CapDescriptor's alignment.
        #[allow(clippy::cast_ptr_alignment)]
        unsafe {
            let ptr = (CHILD_PI_TEMP_VA as *mut u8)
                .add(byte_offset)
                .cast::<CapDescriptor>();
            core::ptr::write(ptr, *desc);
        }
    }

    // Unmap the PI frame.
    let _ = syscall::mem_unmap(info.aspace_cap, CHILD_PI_TEMP_VA, 1);

    // Phase 4: START_PROCESS.
    // SAFETY: writing pid to IPC buffer for the START_PROCESS call.
    unsafe { core::ptr::write_volatile(ipc_buf, pid) };
    match syscall::ipc_call(procmgr_ep, LABEL_START_PROCESS, 1, &[])
    {
        Ok((0, _)) => log("init: devmgr created and started with hardware caps"),
        _ => log("init: devmgr: START_PROCESS failed"),
    }
}

// ── Entry point ──────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn _start(info_ptr: u64) -> !
{
    run(info_ptr)
}

// too_many_lines: bootstrap orchestration is inherently sequential.
#[allow(clippy::too_many_lines)]
fn run(info_ptr: u64) -> !
{
    // SAFETY: kernel maps InitInfo at info_ptr (= INIT_INFO_VADDR).
    // cast_ptr_alignment: INIT_INFO_VADDR is page-aligned.
    #[allow(clippy::cast_ptr_alignment)]
    let info: &InitInfo = unsafe { &*(info_ptr as *const InitInfo) };

    if info.version != INIT_PROTOCOL_VERSION
    {
        // Cannot proceed on version mismatch.
        syscall::thread_exit();
    }

    // Set up serial output.
    arch::serial_init(info, info.thread_cap);
    log("init: starting");

    let mut alloc = FrameAlloc::new(info);

    // Map a fresh page for init's IPC buffer.
    let Some(ipc_cap) = alloc.alloc_page()
    else
    {
        log("init: FATAL: cannot allocate IPC buffer frame");
        syscall::thread_exit();
    };
    if syscall::mem_map(
        ipc_cap,
        info.aspace_cap,
        INIT_IPC_BUF_VA,
        0,
        1,
        syscall::PROT_WRITE,
    )
    .is_err()
    {
        log("init: FATAL: cannot map IPC buffer page");
        syscall::thread_exit();
    }
    // Zero the IPC buffer page.
    // SAFETY: INIT_IPC_BUF_VA is mapped writable, one page.
    unsafe { core::ptr::write_bytes(INIT_IPC_BUF_VA as *mut u8, 0, PAGE_SIZE as usize) };
    if syscall::ipc_buffer_set(INIT_IPC_BUF_VA).is_err()
    {
        log("init: FATAL: ipc_buffer_set failed");
        syscall::thread_exit();
    }
    log("init: IPC buffer registered");

    // ── Bootstrap procmgr ────────────────────────────────────────────────────

    let Some(endpoint_cap) = bootstrap_procmgr(info, &mut alloc)
    else
    {
        log("init: FATAL: failed to bootstrap procmgr");
        syscall::thread_exit();
    };

    // ── Create log endpoint ────────────────────────────────────────────────────
    //
    // Services send log messages via IPC to this endpoint. Init receives them
    // and writes to serial, ensuring atomic output with no interleaving.

    let Ok(log_ep) = syscall::cap_create_endpoint()
    else
    {
        log("init: FATAL: cannot create log endpoint");
        syscall::thread_exit();
    };
    log("init: log endpoint created");

    // ── Create inter-service endpoints ──────────────────────────────────────────

    let Ok(devmgr_registry_ep) = syscall::cap_create_endpoint()
    else
    {
        log("init: FATAL: cannot create devmgr registry endpoint");
        syscall::thread_exit();
    };
    log("init: devmgr registry endpoint created");

    let Ok(vfsd_service_ep) = syscall::cap_create_endpoint()
    else
    {
        log("init: FATAL: cannot create vfsd service endpoint");
        syscall::thread_exit();
    };
    log("init: vfsd service endpoint created");

    // ── Request procmgr to create early services ──────────────────────────────
    //
    // CREATE_PROCESS returns the child in a suspended state. The caller injects
    // caps and patches ProcessInfo, then calls START_PROCESS.

    // SAFETY: INIT_IPC_BUF_VA is the registered IPC buffer, page-aligned.
    #[allow(clippy::cast_ptr_alignment)]
    let ipc_buf = INIT_IPC_BUF_VA as *mut u64;

    if info.module_frame_count >= 2
    {
        log("init: requesting procmgr to create devmgr (with hw caps)");
        create_devmgr_with_caps(info, endpoint_cap, log_ep, devmgr_registry_ep, ipc_buf);
    }
    else
    {
        log("init: no devmgr module available");
    }

    if info.module_frame_count >= 3
    {
        log("init: requesting procmgr to create vfsd (with caps)");
        create_vfsd_with_caps(
            info,
            endpoint_cap,
            log_ep,
            devmgr_registry_ep,
            vfsd_service_ep,
            ipc_buf,
        );
    }
    else
    {
        log("init: no vfsd module available");
    }

    // Spawn log thread so services can log while main thread continues.
    // The log thread needs I/O port access for serial output (x86-64).
    let ioport_cap = find_cap_by_type(info, init_protocol::CapType::IoPortRange).unwrap_or(0);
    spawn_log_thread(info, &mut alloc, log_ep, ioport_cap);

    // Switch main thread from direct serial to IPC-based logging through the
    // log thread. All subsequent log() calls go via IPC — clean, serialized.
    // SAFETY: single main thread; log thread only reads its own log_ep argument.
    unsafe {
        LOG_EP_SLOT = log_ep;
        MAIN_IPC_BUF = ipc_buf;
    }
    log("init: log thread started");

    // Phase 1 bootstrap complete — services are running, log thread active.
    log("init: phase 1 bootstrap complete");

    // ── Phase 2: mount root filesystem ──────────────────────────────────────
    //
    // Parse kernel cmdline for root=UUID=<guid>, send MOUNT to vfsd.

    // SAFETY: InitInfo page is valid and contains cmdline data.
    let cmdline = unsafe { init_protocol::cmdline_bytes(info) };
    log("init: phase 2: parsing cmdline");

    let mut root_uuid = [0u8; 16];
    if !parse_root_uuid(cmdline, &mut root_uuid)
    {
        log("init: FATAL: no root=UUID= in cmdline");
        syscall::thread_exit();
    }

    log("init: phase 2: mounting root filesystem");
    if !send_mount(vfsd_service_ep, ipc_buf, &root_uuid, b"/")
    {
        log("init: FATAL: root mount failed");
        syscall::thread_exit();
    }
    log("init: phase 2: root mounted at /");

    // ── Phase 2b: read /config/mounts.conf, mount additional filesystems ────

    log("init: phase 2: reading /config/mounts.conf");
    let mut conf_buf = [0u8; 512];
    let conf_len = vfs_read_file(
        vfsd_service_ep,
        ipc_buf,
        b"/config/mounts.conf",
        &mut conf_buf,
    );

    if conf_len > 0
    {
        log("init: phase 2: processing mounts.conf");
        process_mounts_conf(&conf_buf[..conf_len], vfsd_service_ep, ipc_buf);
    }
    else
    {
        log("init: phase 2: no mounts.conf or empty");
    }

    // End-to-end verification: read a file across the multi-mount namespace.
    // /esp/EFI/seraph/boot.conf goes through the ESP mount at /esp.
    log("init: phase 2: verifying /esp/EFI/seraph/boot.conf");
    let mut verify_buf = [0u8; 512];
    let verify_len = vfs_read_file(
        vfsd_service_ep,
        ipc_buf,
        b"/esp/EFI/seraph/boot.conf",
        &mut verify_buf,
    );
    if verify_len > 0
    {
        // Print the first line of boot.conf as proof of end-to-end read.
        let first_nl = verify_buf[..verify_len]
            .iter()
            .position(|&b| b == b'\n')
            .unwrap_or(verify_len);
        let line = &verify_buf[..first_nl.min(80)];
        // SAFETY: boot.conf is ASCII text.
        let s = unsafe { core::str::from_utf8_unchecked(line) };
        log(s);
    }
    else
    {
        log("init: phase 2: boot.conf read FAILED");
    }

    log("init: phase 2 bootstrap complete");

    // ── Phase 3: svcmgr, service registration, handover ────────────────────

    phase3_svcmgr_handover(info, endpoint_cap, log_ep, vfsd_service_ep, ipc_buf);
}

// ── Phase 3: svcmgr creation, service registration, handover ──────────────

/// Send `SET_VFSD_ENDPOINT` to procmgr so it can do VFS-based ELF loading.
fn send_vfsd_endpoint_to_procmgr(procmgr_ep: u32, vfsd_ep: u32)
{
    let Ok(vfsd_copy) = syscall::cap_derive(vfsd_ep, !0u64)
    else
    {
        log("init: phase 3: failed to derive vfsd endpoint");
        return;
    };
    match syscall::ipc_call(procmgr_ep, LABEL_SET_VFSD_EP, 0, &[vfsd_copy])
    {
        Ok((0, _)) => log("init: phase 3: vfsd endpoint sent to procmgr"),
        _ => log("init: phase 3: SET_VFSD_ENDPOINT failed"),
    }
}

/// Create svcmgr from VFS (`/bin/svcmgr`) via `CREATE_PROCESS_FROM_VFS`.
///
/// Returns `(pid, child_cspace, pi_frame, thread_cap)` on success.
fn create_svcmgr_from_vfs(procmgr_ep: u32, ipc_buf: *mut u64) -> Option<(u64, u32, u32, u32)>
{
    let path = b"/bin/svcmgr";
    let path_len = path.len();

    // Pack path into IPC buffer data words.
    let word_count = path_len.div_ceil(8);
    for w in 0..word_count
    {
        let mut word: u64 = 0;
        for b in 0..8
        {
            let idx = w * 8 + b;
            if idx < path_len
            {
                word |= u64::from(path[idx]) << (b * 8);
            }
        }
        // SAFETY: ipc_buf is the registered IPC buffer.
        unsafe { core::ptr::write_volatile(ipc_buf.add(w), word) };
    }

    let label = LABEL_CREATE_FROM_VFS | ((path_len as u64) << 16);
    let Ok((reply_label, _)) = syscall::ipc_call(procmgr_ep, label, word_count, &[])
    else
    {
        log("init: phase 3: CREATE_PROCESS_FROM_VFS ipc_call failed");
        return None;
    };
    if reply_label != 0
    {
        log("init: phase 3: CREATE_PROCESS_FROM_VFS failed");
        return None;
    }

    // SAFETY: ipc_buf is the registered IPC buffer.
    let pid = unsafe { core::ptr::read_volatile(ipc_buf) };
    // SAFETY: ipc_buf is the registered IPC buffer, page-aligned.
    #[allow(clippy::cast_ptr_alignment)]
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc_buf) };
    if cap_count < 3
    {
        log("init: phase 3: svcmgr reply missing caps");
        return None;
    }

    Some((pid, reply_caps[0], reply_caps[1], reply_caps[2]))
}

/// Inject caps into svcmgr's `CSpace` and patch `ProcessInfo`, then start it.
#[allow(clippy::too_many_arguments)]
fn setup_and_start_svcmgr(
    info: &InitInfo,
    procmgr_ep: u32,
    log_ep: u32,
    svcmgr_service_ep: u32,
    pid: u64,
    child_cspace: u32,
    pi_frame: u32,
    ipc_buf: *mut u64,
)
{
    let mut desc_buf = [CapDescriptor {
        slot: 0,
        cap_type: CapType::Frame,
        pad: [0; 3],
        aux0: 0,
        aux1: 0,
    }; 8];
    let mut desc_count: usize = 0;
    let mut first_slot: u32 = 0;

    // Log endpoint.
    inject_cap_desc(
        log_ep,
        CapType::Frame,
        LOG_ENDPOINT_SENTINEL,
        0,
        child_cspace,
        &mut desc_buf,
        &mut desc_count,
        &mut first_slot,
    );

    // Service endpoint (svcmgr receives registrations on this).
    inject_cap_desc(
        svcmgr_service_ep,
        CapType::Frame,
        SERVICE_ENDPOINT_SENTINEL,
        0,
        child_cspace,
        &mut desc_buf,
        &mut desc_count,
        &mut first_slot,
    );

    // procmgr endpoint (svcmgr uses this for restarting services).
    inject_cap_desc(
        procmgr_ep,
        CapType::Frame,
        PROCMGR_ENDPOINT_SENTINEL,
        0,
        child_cspace,
        &mut desc_buf,
        &mut desc_count,
        &mut first_slot,
    );

    // Patch ProcessInfo.
    if syscall::mem_map(
        pi_frame,
        info.aspace_cap,
        CHILD_PI_TEMP_VA,
        0,
        1,
        syscall::PROT_WRITE,
    )
    .is_err()
    {
        log("init: phase 3: cannot map svcmgr ProcessInfo");
        return;
    }

    // SAFETY: CHILD_PI_TEMP_VA is mapped writable to the ProcessInfo page.
    #[allow(clippy::cast_ptr_alignment)]
    let pi = unsafe { &mut *(CHILD_PI_TEMP_VA as *mut ProcessInfo) };

    pi.initial_caps_base = first_slot;
    pi.initial_caps_count = desc_count as u32;
    pi.cap_descriptor_count = desc_count as u32;

    let descs_offset = core::mem::size_of::<ProcessInfo>() as u32;
    let descs_offset_aligned = (descs_offset + 7) & !7;
    pi.cap_descriptors_offset = descs_offset_aligned;

    let desc_size = core::mem::size_of::<CapDescriptor>();
    for (i, desc) in desc_buf.iter().enumerate().take(desc_count)
    {
        let byte_offset = descs_offset_aligned as usize + i * desc_size;
        if byte_offset + desc_size > PAGE_SIZE as usize
        {
            break;
        }
        // SAFETY: byte_offset is within the mapped page.
        #[allow(clippy::cast_ptr_alignment)]
        unsafe {
            let ptr = (CHILD_PI_TEMP_VA as *mut u8)
                .add(byte_offset)
                .cast::<CapDescriptor>();
            core::ptr::write(ptr, *desc);
        }
    }

    let _ = syscall::mem_unmap(info.aspace_cap, CHILD_PI_TEMP_VA, 1);

    // START_PROCESS.
    // SAFETY: writing pid to IPC buffer.
    unsafe { core::ptr::write_volatile(ipc_buf, pid) };
    match syscall::ipc_call(procmgr_ep, LABEL_START_PROCESS, 1, &[])
    {
        Ok((0, _)) => log("init: phase 3: svcmgr started"),
        _ => log("init: phase 3: svcmgr START_PROCESS failed"),
    }
}

/// Create crasher from its boot module (suspended). Returns `(pid, thread_cap, module_cap)`.
fn create_crasher_suspended(
    info: &InitInfo,
    procmgr_ep: u32,
    log_ep: u32,
    ipc_buf: *mut u64,
) -> Option<(u64, u32, u32)>
{
    // Crasher is module index 5 (procmgr=0, devmgr=1, vfsd=2, virtio-blk=3, fatfs=4, crasher=5).
    if info.module_frame_count < 6
    {
        log("init: phase 3: no crasher module available");
        return None;
    }

    let crasher_frame_cap = info.module_frame_base + 5;

    // Derive a copy for procmgr's CREATE_PROCESS (IPC moves caps).
    // Keep the original for svcmgr's restart recipe.
    let Ok(frame_for_procmgr) = syscall::cap_derive(crasher_frame_cap, !0u64)
    else
    {
        log("init: phase 3: cannot derive crasher module cap");
        return None;
    };

    let Ok((reply_label, _)) =
        syscall::ipc_call(procmgr_ep, LABEL_CREATE_PROCESS, 0, &[frame_for_procmgr])
    else
    {
        log("init: phase 3: crasher CREATE_PROCESS failed");
        return None;
    };
    if reply_label != 0
    {
        log("init: phase 3: crasher CREATE_PROCESS error");
        return None;
    }

    // SAFETY: ipc_buf is the registered IPC buffer.
    let pid = unsafe { core::ptr::read_volatile(ipc_buf) };
    // SAFETY: ipc_buf is the registered IPC buffer, page-aligned.
    #[allow(clippy::cast_ptr_alignment)]
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc_buf) };
    if cap_count < 3
    {
        log("init: phase 3: crasher reply missing caps");
        return None;
    }
    let child_cspace = reply_caps[0];
    let pi_frame = reply_caps[1];
    let thread_cap = reply_caps[2];

    // Inject log endpoint into crasher.
    let mut desc_buf = [CapDescriptor {
        slot: 0,
        cap_type: CapType::Frame,
        pad: [0; 3],
        aux0: 0,
        aux1: 0,
    }; 4];
    let mut desc_count: usize = 0;
    let mut first_slot: u32 = 0;

    inject_cap_desc(
        log_ep,
        CapType::Frame,
        LOG_ENDPOINT_SENTINEL,
        0,
        child_cspace,
        &mut desc_buf,
        &mut desc_count,
        &mut first_slot,
    );

    // Patch ProcessInfo.
    if syscall::mem_map(
        pi_frame,
        info.aspace_cap,
        CHILD_PI_TEMP_VA,
        0,
        1,
        syscall::PROT_WRITE,
    )
    .is_err()
    {
        log("init: phase 3: cannot map crasher ProcessInfo");
        return None;
    }

    // SAFETY: CHILD_PI_TEMP_VA is mapped writable to the ProcessInfo page.
    #[allow(clippy::cast_ptr_alignment)]
    let pi = unsafe { &mut *(CHILD_PI_TEMP_VA as *mut ProcessInfo) };

    pi.initial_caps_base = first_slot;
    pi.initial_caps_count = desc_count as u32;
    pi.cap_descriptor_count = desc_count as u32;

    let descs_offset = core::mem::size_of::<ProcessInfo>() as u32;
    let descs_offset_aligned = (descs_offset + 7) & !7;
    pi.cap_descriptors_offset = descs_offset_aligned;

    let desc_size = core::mem::size_of::<CapDescriptor>();
    for (i, desc) in desc_buf.iter().enumerate().take(desc_count)
    {
        let byte_offset = descs_offset_aligned as usize + i * desc_size;
        if byte_offset + desc_size > PAGE_SIZE as usize
        {
            break;
        }
        // SAFETY: byte_offset is within the mapped page.
        #[allow(clippy::cast_ptr_alignment)]
        unsafe {
            let ptr = (CHILD_PI_TEMP_VA as *mut u8)
                .add(byte_offset)
                .cast::<CapDescriptor>();
            core::ptr::write(ptr, *desc);
        }
    }

    let _ = syscall::mem_unmap(info.aspace_cap, CHILD_PI_TEMP_VA, 1);

    // Do NOT start — svcmgr must bind death notification before crasher runs.
    log("init: phase 3: crasher created (suspended)");
    Some((pid, thread_cap, crasher_frame_cap))
}

/// Register a service with svcmgr via `REGISTER_SERVICE`.
///
/// Sends name, policy, criticality in data words and up to 3 caps
/// (thread, module, `log_ep`) in cap slots.
#[allow(clippy::too_many_arguments)]
fn register_service(
    svcmgr_ep: u32,
    ipc_buf: *mut u64,
    name: &[u8],
    restart_policy: u8,
    criticality: u8,
    thread_cap: u32,
    module_cap: u32,
    log_ep: u32,
)
{
    // data[0] = restart_policy, data[1] = criticality, data[2..] = name packed.
    // SAFETY: ipc_buf is the registered IPC buffer.
    unsafe { core::ptr::write_volatile(ipc_buf, u64::from(restart_policy)) };
    // SAFETY: ipc_buf offset 1.
    unsafe { core::ptr::write_volatile(ipc_buf.add(1), u64::from(criticality)) };

    let name_words = name.len().div_ceil(8);
    for w in 0..name_words
    {
        let mut word: u64 = 0;
        for b in 0..8
        {
            let idx = w * 8 + b;
            if idx < name.len()
            {
                word |= u64::from(name[idx]) << (b * 8);
            }
        }
        // SAFETY: ipc_buf is valid; writing name word at offset 2+w.
        unsafe { core::ptr::write_volatile(ipc_buf.add(2 + w), word) };
    }

    let data_count = 2 + name_words;
    let label = LABEL_REGISTER_SERVICE | ((name.len() as u64) << 16);

    // Build cap list. For Fatal services, module_cap and log_ep may be 0.
    let mut caps = [0u32; 3];
    let mut cap_count = 0;
    if thread_cap != 0
    {
        caps[cap_count] = thread_cap;
        cap_count += 1;
    }
    if module_cap != 0
    {
        caps[cap_count] = module_cap;
        cap_count += 1;
    }
    if log_ep != 0 && module_cap != 0
    {
        // Only send log_ep if service is restartable (has module_cap).
        if let Ok(derived) = syscall::cap_derive(log_ep, !0u64)
        {
            caps[cap_count] = derived;
            cap_count += 1;
        }
    }

    match syscall::ipc_call(svcmgr_ep, label, data_count, &caps[..cap_count])
    {
        Ok((0, _)) =>
        {}
        _ => log("init: phase 3: REGISTER_SERVICE failed"),
    }
}

/// Phase 3: create svcmgr from VFS, register services, start crasher, handover.
#[allow(clippy::too_many_lines)]
fn phase3_svcmgr_handover(
    info: &InitInfo,
    procmgr_ep: u32,
    log_ep: u32,
    vfsd_service_ep: u32,
    ipc_buf: *mut u64,
) -> !
{
    // 1. Send vfsd endpoint to procmgr for VFS-based ELF loading.
    send_vfsd_endpoint_to_procmgr(procmgr_ep, vfsd_service_ep);

    // 2. Create svcmgr service endpoint.
    let Ok(svcmgr_service_ep) = syscall::cap_create_endpoint()
    else
    {
        log("init: phase 3: cannot create svcmgr endpoint");
        idle_loop();
    };

    // 3. Create svcmgr from VFS.
    log("init: phase 3: loading svcmgr from /bin/svcmgr");
    let Some((svcmgr_pid, svcmgr_cspace, svcmgr_pi, _svcmgr_thread)) =
        create_svcmgr_from_vfs(procmgr_ep, ipc_buf)
    else
    {
        log("init: phase 3: failed to create svcmgr, idling");
        idle_loop();
    };

    // 4. Inject caps and start svcmgr.
    setup_and_start_svcmgr(
        info,
        procmgr_ep,
        log_ep,
        svcmgr_service_ep,
        svcmgr_pid,
        svcmgr_cspace,
        svcmgr_pi,
        ipc_buf,
    );

    // 5. Create crasher (suspended — don't start until svcmgr is monitoring).
    let crasher = create_crasher_suspended(info, procmgr_ep, log_ep, ipc_buf);

    // 6. Register services with svcmgr.
    log("init: phase 3: registering services with svcmgr");

    // procmgr: Fatal, Never restart. Thread cap not available from bootstrap_procmgr
    // (it was created via raw syscalls, not procmgr IPC). Skip for now — procmgr
    // crash is unrecoverable regardless.

    // crasher: Normal, Always restart.
    if let Some((_, crasher_thread, crasher_module)) = crasher
    {
        register_service(
            svcmgr_service_ep,
            ipc_buf,
            b"crasher",
            0, // POLICY_ALWAYS
            1, // CRITICALITY_NORMAL
            crasher_thread,
            crasher_module,
            log_ep,
        );

        // 7. Start crasher now that svcmgr is monitoring.
        // SAFETY: writing pid to IPC buffer.
        unsafe { core::ptr::write_volatile(ipc_buf, crasher.unwrap().0) };
        match syscall::ipc_call(procmgr_ep, LABEL_START_PROCESS, 1, &[])
        {
            Ok((0, _)) => log("init: phase 3: crasher started"),
            _ => log("init: phase 3: crasher START_PROCESS failed"),
        }
    }

    // 8. HANDOVER_COMPLETE.
    match syscall::ipc_call(svcmgr_service_ep, LABEL_HANDOVER_COMPLETE, 0, &[])
    {
        Ok((0, _)) => log("init: phase 3: handover complete"),
        _ => log("init: phase 3: handover failed"),
    }

    log("init: main thread exiting, log thread continues");
    syscall::thread_exit();
}

/// Idle loop fallback when Phase 3 cannot proceed.
fn idle_loop() -> !
{
    loop
    {
        let _ = syscall::thread_yield();
    }
}

// ── Log receive loop ───────────────────────────────────────────────────────

// ── VFS client helpers ────────────────────────────────────────────────────────

/// VFS IPC labels (must match vfsd).
const VFS_LABEL_OPEN: u64 = 1;
const VFS_LABEL_READ: u64 = 2;
const VFS_LABEL_CLOSE: u64 = 3;
const VFS_LABEL_MOUNT: u64 = 10;

/// Parse `root=UUID=<uuid>` from kernel cmdline bytes.
///
/// UUID format: `12345678-abcd-ef01-2345-6789abcdef01` (36 chars).
/// Converts to 16-byte mixed-endian GPT format.
fn parse_root_uuid(cmdline: &[u8], out: &mut [u8; 16]) -> bool
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

/// Send a MOUNT IPC request to vfsd.
///
/// MOUNT data layout: `data[0..2]` = UUID, `data[2]` = `path_len`,
/// `data[3..]` = path.
fn send_mount(vfsd_ep: u32, ipc_buf: *mut u64, uuid: &[u8; 16], path: &[u8]) -> bool
{
    // Pack UUID into data[0..2].
    let w0 = u64::from_le_bytes(uuid[..8].try_into().unwrap_or([0; 8]));
    let w1 = u64::from_le_bytes(uuid[8..].try_into().unwrap_or([0; 8]));
    // SAFETY: IPC buffer is valid.
    unsafe {
        core::ptr::write_volatile(ipc_buf, w0);
        core::ptr::write_volatile(ipc_buf.add(1), w1);
        core::ptr::write_volatile(ipc_buf.add(2), path.len() as u64);
    }

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
        // SAFETY: IPC buffer is valid.
        unsafe { core::ptr::write_volatile(ipc_buf.add(3 + i), word) };
    }

    let total_words = 3 + word_count;
    let Ok((reply_label, _)) = syscall::ipc_call(vfsd_ep, VFS_LABEL_MOUNT, total_words, &[])
    else
    {
        return false;
    };
    reply_label == 0
}

/// Read a file from the VFS into a buffer. Returns bytes read (0 on error).
fn vfs_read_file(vfsd_ep: u32, ipc_buf: *mut u64, path: &[u8], buf: &mut [u8; 512]) -> usize
{
    // OPEN
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
        // SAFETY: IPC buffer is valid.
        unsafe { core::ptr::write_volatile(ipc_buf.add(i), word) };
    }

    let open_label = VFS_LABEL_OPEN | ((path.len() as u64) << 16);
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

    // SAFETY: IPC buffer is valid.
    let fd = unsafe { core::ptr::read_volatile(ipc_buf) };

    // READ (up to 512 bytes at offset 0)
    // SAFETY: IPC buffer is valid.
    unsafe {
        core::ptr::write_volatile(ipc_buf, fd);
        core::ptr::write_volatile(ipc_buf.add(1), 0); // offset
        core::ptr::write_volatile(ipc_buf.add(2), 512); // max_len
    }

    let Ok((reply_label, data_count)) = syscall::ipc_call(vfsd_ep, VFS_LABEL_READ, 3, &[])
    else
    {
        log("init: vfs_read: READ failed");
        return 0;
    };
    let _ = data_count; // ipc_call always returns data_count=0; unused.
    if reply_label != 0
    {
        log("init: vfs_read: READ error");
        return 0;
    }

    // data[0] = bytes_read, data[1..] = content.
    // Copy BEFORE any log() calls (IPC buffer shared).
    // SAFETY: IPC buffer is valid.
    let bytes_read = unsafe { core::ptr::read_volatile(ipc_buf) } as usize;
    // Derive content word count from bytes_read (ipc_call doesn't return it).
    let content_words = bytes_read.div_ceil(8);
    for i in 0..content_words
    {
        // SAFETY: IPC buffer is valid.
        let word = unsafe { core::ptr::read_volatile(ipc_buf.add(1 + i)) };
        let base = i * 8;
        for j in 0..8
        {
            if base + j < bytes_read && base + j < buf.len()
            {
                buf[base + j] = ((word >> (j * 8)) & 0xFF) as u8;
            }
        }
    }

    // CLOSE
    // SAFETY: IPC buffer is valid.
    unsafe { core::ptr::write_volatile(ipc_buf, fd) };
    let _ = syscall::ipc_call(vfsd_ep, VFS_LABEL_CLOSE, 1, &[]);

    bytes_read
}

/// Parse `mounts.conf` and issue MOUNT requests for each entry.
///
/// Format: one mount per line, `UUID=<uuid> <path> <fstype>`.
/// Lines starting with `#` are comments. Empty lines are skipped.
fn process_mounts_conf(data: &[u8], vfsd_ep: u32, ipc_buf: *mut u64)
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

// ── Log thread ────────────────────────────────────────────────────────────────

/// Spawn a dedicated log-receiving thread so the main thread can continue
/// bootstrap orchestration (making IPC calls to vfsd etc.) without blocking
/// service log output.
fn spawn_log_thread(info: &InitInfo, alloc: &mut FrameAlloc, log_ep: u32, ioport_cap: u32)
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
        let Ok(rw_cap) = syscall::cap_derive(frame, 0x1 | 0x2)
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
    let Ok(ipc_rw_cap) = syscall::cap_derive(ipc_frame, 0x1 | 0x2)
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

// ── Log receive loop ──────────────────────────────────────────────────────

/// Log label base (bits 0-15). Must match `runtime::log::LOG_LABEL_BASE`.
const LOG_LABEL_BASE: u64 = 10;

/// Continuation flag (bit 32). Must match `runtime::log::LOG_CONTINUATION`.
const LOG_CONTINUATION: u64 = 1 << 32;

/// Max bytes per IPC chunk.
const LOG_CHUNK_SIZE: usize = 6 * 8; // MSG_DATA_WORDS_MAX * 8

/// Maximum assembled log message length (multiple chunks).
const LOG_MAX_ASSEMBLED: usize = 256;

/// Receive log messages from services and write them to serial.
///
/// Handles multi-chunk messages: chunks with the continuation flag set are
/// accumulated into `assembled_buf`. When the final chunk arrives (no flag),
/// the complete message is written to serial with CRLF.
fn log_receive_loop(log_ep: u32, ipc_buf: *mut u64) -> !
{
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
                // SAFETY: IPC buffer is valid; i < MSG_DATA_WORDS_MAX.
                let word = unsafe { core::ptr::read_volatile(ipc_buf.add(i)) };
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
                        arch::serial_write_byte(b'\r');
                    }
                    arch::serial_write_byte(b);
                }
                arch::serial_write_byte(b'\r');
                arch::serial_write_byte(b'\n');
                assembled_len = 0;
            }
        }

        // Reply to unblock the sender.
        let _ = syscall::ipc_reply(0, 0, &[]);
    }
}

// ── Service creation with log endpoint ─────────────────────────────────────

/// Create a service via procmgr with a log endpoint cap injected.
///
/// Two-phase creation: `CREATE_PROCESS` (suspended), inject log endpoint cap
/// into child `CSpace`, patch `ProcessInfo` with `CapDescriptor`, then
/// `START_PROCESS`.
#[allow(clippy::too_many_arguments, dead_code)]
fn create_and_start_service_with_log(
    info: &InitInfo,
    procmgr_ep: u32,
    module_frame_cap: u32,
    log_ep: u32,
    ipc_buf: *mut u64,
    ok_msg: &str,
    fail_msg: &str,
)
{
    // Phase 1: CREATE_PROCESS (suspended).
    let Ok((reply_label, _)) =
        syscall::ipc_call(procmgr_ep, LABEL_CREATE_PROCESS, 0, &[module_frame_cap])
    else
    {
        log(fail_msg);
        return;
    };
    if reply_label != 0
    {
        log(fail_msg);
        return;
    }

    // Read pid and child caps.
    // SAFETY: IPC buffer is valid.
    let pid = unsafe { core::ptr::read_volatile(ipc_buf) };
    // SAFETY: ipc_buf is the registered IPC buffer.
    #[allow(clippy::cast_ptr_alignment)]
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc_buf) };
    if cap_count < 2
    {
        log(fail_msg);
        return;
    }
    let child_cspace = reply_caps[0];
    let pi_frame = reply_caps[1];

    // Phase 2: Inject log endpoint cap.
    let mut desc_buf = [CapDescriptor {
        slot: 0,
        cap_type: CapType::Frame,
        pad: [0; 3],
        aux0: 0,
        aux1: 0,
    }; 4];
    let mut desc_count: usize = 0;
    let mut first_slot: u32 = 0;

    inject_cap_desc(
        log_ep,
        CapType::Frame,
        LOG_ENDPOINT_SENTINEL,
        0,
        child_cspace,
        &mut desc_buf,
        &mut desc_count,
        &mut first_slot,
    );

    // Phase 3: Patch ProcessInfo.
    if syscall::mem_map(
        pi_frame,
        info.aspace_cap,
        CHILD_PI_TEMP_VA,
        0,
        1,
        syscall::PROT_WRITE,
    )
    .is_err()
    {
        log(fail_msg);
        return;
    }

    // SAFETY: CHILD_PI_TEMP_VA is mapped writable to the ProcessInfo page.
    #[allow(clippy::cast_ptr_alignment)]
    let pi = unsafe { &mut *(CHILD_PI_TEMP_VA as *mut ProcessInfo) };

    pi.initial_caps_base = first_slot;
    pi.initial_caps_count = desc_count as u32;
    pi.cap_descriptor_count = desc_count as u32;

    let descs_offset = core::mem::size_of::<ProcessInfo>() as u32;
    let descs_offset_aligned = (descs_offset + 7) & !7;
    pi.cap_descriptors_offset = descs_offset_aligned;

    let desc_size = core::mem::size_of::<CapDescriptor>();
    for (i, desc) in desc_buf.iter().enumerate().take(desc_count)
    {
        let byte_offset = descs_offset_aligned as usize + i * desc_size;
        if byte_offset + desc_size > PAGE_SIZE as usize
        {
            break;
        }
        // SAFETY: byte_offset is within the mapped page.
        #[allow(clippy::cast_ptr_alignment)]
        unsafe {
            let ptr = (CHILD_PI_TEMP_VA as *mut u8)
                .add(byte_offset)
                .cast::<CapDescriptor>();
            core::ptr::write(ptr, *desc);
        }
    }

    let _ = syscall::mem_unmap(info.aspace_cap, CHILD_PI_TEMP_VA, 1);

    // Phase 4: START_PROCESS.
    // SAFETY: writing pid to IPC buffer.
    unsafe { core::ptr::write_volatile(ipc_buf, pid) };
    match syscall::ipc_call(procmgr_ep, LABEL_START_PROCESS, 1, &[])
    {
        Ok((0, _)) => log(ok_msg),
        _ => log(fail_msg),
    }
}

// ── vfsd creation with full cap delegation ──────────────────────────────────

/// Create vfsd with caps needed for filesystem support.
///
/// Two-phase creation: `CREATE_PROCESS` (suspended), inject caps (log,
/// procmgr, devmgr registry, fatfs module, service endpoint), write
/// startup message with mount config, then `START_PROCESS`.
// too_many_lines: cap delegation is inherently sequential; splitting would
// fragment the delegation flow.
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
fn create_vfsd_with_caps(
    info: &InitInfo,
    procmgr_ep: u32,
    log_ep: u32,
    registry_ep: u32,
    vfsd_service_ep: u32,
    ipc_buf: *mut u64,
)
{
    let vfsd_frame_cap = info.module_frame_base + 2; // Module 2 = vfsd

    // Phase 1: CREATE_PROCESS (suspended).
    let Ok((reply_label, _)) =
        syscall::ipc_call(procmgr_ep, LABEL_CREATE_PROCESS, 0, &[vfsd_frame_cap])
    else
    {
        log("init: vfsd: CREATE_PROCESS ipc_call failed");
        return;
    };
    if reply_label != 0
    {
        log("init: vfsd: CREATE_PROCESS failed");
        return;
    }

    // SAFETY: IPC buffer is valid and kernel wrote reply data.
    let pid = unsafe { core::ptr::read_volatile(ipc_buf) };
    // SAFETY: ipc_buf is the registered IPC buffer.
    #[allow(clippy::cast_ptr_alignment)]
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc_buf) };
    if cap_count < 2
    {
        log("init: vfsd: CREATE_PROCESS reply missing caps");
        return;
    }
    let child_cspace = reply_caps[0];
    let pi_frame = reply_caps[1];

    // Phase 2: Inject caps.
    let mut desc_buf = [CapDescriptor {
        slot: 0,
        cap_type: CapType::Frame,
        pad: [0; 3],
        aux0: 0,
        aux1: 0,
    }; VFSD_MAX_DESCS];
    let mut desc_count: usize = 0;
    let mut first_slot: u32 = 0;

    // Log endpoint.
    inject_cap_desc(
        log_ep,
        CapType::Frame,
        LOG_ENDPOINT_SENTINEL,
        0,
        child_cspace,
        &mut desc_buf,
        &mut desc_count,
        &mut first_slot,
    );

    // procmgr endpoint (sentinel: aux0=0, aux1=0).
    inject_cap_desc(
        procmgr_ep,
        CapType::Frame,
        0,
        0,
        child_cspace,
        &mut desc_buf,
        &mut desc_count,
        &mut first_slot,
    );

    // devmgr registry endpoint (send cap).
    inject_cap_desc(
        registry_ep,
        CapType::Frame,
        REGISTRY_ENDPOINT_SENTINEL,
        0,
        child_cspace,
        &mut desc_buf,
        &mut desc_count,
        &mut first_slot,
    );

    // fatfs module frame cap (module 4).
    if info.module_frame_count > 4
    {
        let fatfs_cap = info.module_frame_base + 4;
        inject_cap_desc(
            fatfs_cap,
            CapType::Frame,
            4, // aux0 = module index
            0,
            child_cspace,
            &mut desc_buf,
            &mut desc_count,
            &mut first_slot,
        );
    }

    // vfsd service endpoint (receive cap).
    inject_cap_desc(
        vfsd_service_ep,
        CapType::Frame,
        SERVICE_ENDPOINT_SENTINEL,
        0,
        child_cspace,
        &mut desc_buf,
        &mut desc_count,
        &mut first_slot,
    );

    // Phase 3: Patch ProcessInfo page.
    if syscall::mem_map(
        pi_frame,
        info.aspace_cap,
        CHILD_PI_TEMP_VA,
        0,
        1,
        syscall::PROT_WRITE,
    )
    .is_err()
    {
        log("init: vfsd: cannot map ProcessInfo frame");
        return;
    }

    // SAFETY: CHILD_PI_TEMP_VA is mapped writable to the ProcessInfo page.
    #[allow(clippy::cast_ptr_alignment)]
    let pi = unsafe { &mut *(CHILD_PI_TEMP_VA as *mut ProcessInfo) };

    pi.initial_caps_base = first_slot;
    pi.initial_caps_count = desc_count as u32;
    pi.cap_descriptor_count = desc_count as u32;

    let descs_offset = core::mem::size_of::<ProcessInfo>() as u32;
    let descs_offset_aligned = (descs_offset + 7) & !7;
    pi.cap_descriptors_offset = descs_offset_aligned;

    // Write CapDescriptor entries.
    let desc_size = core::mem::size_of::<CapDescriptor>();
    for (i, desc) in desc_buf.iter().enumerate().take(desc_count)
    {
        let byte_offset = descs_offset_aligned as usize + i * desc_size;
        if byte_offset + desc_size > PAGE_SIZE as usize
        {
            break;
        }
        // SAFETY: byte_offset is within the mapped page; alignment is correct.
        #[allow(clippy::cast_ptr_alignment)]
        unsafe {
            let ptr = (CHILD_PI_TEMP_VA as *mut u8)
                .add(byte_offset)
                .cast::<CapDescriptor>();
            core::ptr::write(ptr, *desc);
        }
    }

    let _ = syscall::mem_unmap(info.aspace_cap, CHILD_PI_TEMP_VA, 1);

    // Phase 4: START_PROCESS.
    // SAFETY: writing pid to IPC buffer.
    unsafe { core::ptr::write_volatile(ipc_buf, pid) };
    match syscall::ipc_call(procmgr_ep, LABEL_START_PROCESS, 1, &[])
    {
        Ok((0, _)) => log("init: vfsd created and started with caps"),
        _ => log("init: vfsd: START_PROCESS failed"),
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> !
{
    log("init: PANIC");
    syscall::thread_exit();
}
