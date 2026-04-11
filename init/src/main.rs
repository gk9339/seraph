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

/// IPC label for `CREATE_PROCESS` (per `procmgr/docs/ipc-interface.md`).
const LABEL_CREATE_PROCESS: u64 = 1;

/// IPC label for `START_PROCESS` (per `procmgr/docs/ipc-interface.md`).
const LABEL_START_PROCESS: u64 = 2;

// (IPC buffer is explicitly mapped at INIT_IPC_BUF_VA using a fresh frame.)

// ── Architecture-specific code (serial output, ELF machine type) ─────────────

mod arch;

fn log(s: &str)
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
    let pm_cspace = syscall::cap_create_cspace(1024).ok()?;
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

// ── Two-phase service creation ───────────────────────────────────────────────

/// Create a service via procmgr and immediately start it (no cap injection).
///
/// Used for services that don't need initial caps beyond their identity caps
/// (e.g., vfsd in Tier 2). Services that need hardware caps (e.g., devmgr in
/// Tier 3) should use `CREATE_PROCESS` + cap injection + `START_PROCESS`
/// separately.
fn create_and_start_service(
    procmgr_ep: u32,
    module_frame_cap: u32,
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

    // Read pid from IPC buffer data[0].
    // SAFETY: IPC buffer is valid and kernel wrote reply data.
    let pid = unsafe { core::ptr::read_volatile(ipc_buf) };

    // Phase 2: START_PROCESS.
    // SAFETY: writing pid to IPC buffer for the START_PROCESS call.
    unsafe { core::ptr::write_volatile(ipc_buf, pid) };
    match syscall::ipc_call(procmgr_ep, LABEL_START_PROCESS, 1, &[])
    {
        Ok((0, _)) => log(ok_msg),
        _ => log(fail_msg),
    }
}

// ── Entry point ──────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn _start(info_ptr: u64) -> !
{
    run(info_ptr)
}

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

    // ── Request procmgr to create early services ──────────────────────────────
    //
    // CREATE_PROCESS returns the child in a suspended state. The caller injects
    // caps and patches ProcessInfo, then calls START_PROCESS.

    // SAFETY: INIT_IPC_BUF_VA is the registered IPC buffer, page-aligned.
    #[allow(clippy::cast_ptr_alignment)]
    let ipc_buf = INIT_IPC_BUF_VA as *mut u64;

    if info.module_frame_count >= 2
    {
        let devmgr_frame_cap = info.module_frame_base + 1; // Module 1 = devmgr
        log("init: requesting procmgr to create devmgr");
        create_and_start_service(
            endpoint_cap,
            devmgr_frame_cap,
            ipc_buf,
            "init: devmgr created and started",
            "init: FAILED to create/start devmgr",
        );
    }
    else
    {
        log("init: no devmgr module available");
    }

    if info.module_frame_count >= 3
    {
        let vfsd_frame_cap = info.module_frame_base + 2; // Module 2 = vfsd
        log("init: requesting procmgr to create vfsd");
        create_and_start_service(
            endpoint_cap,
            vfsd_frame_cap,
            ipc_buf,
            "init: vfsd created and started",
            "init: FAILED to create/start vfsd",
        );
    }
    else
    {
        log("init: no vfsd module available");
    }

    // Phase 1 bootstrap complete. Init stays resident for future bootstrap
    // stages (Tier 3+: hardware cap delegation, real root mount, svcmgr handover).
    log("init: phase 1 bootstrap complete, waiting");
    loop
    {
        let _ = syscall::thread_yield();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> !
{
    log("init: PANIC");
    syscall::thread_exit();
}
