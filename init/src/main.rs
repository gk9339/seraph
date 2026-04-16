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
// cast_possible_truncation: init targets 64-bit only; u64/usize conversions
// are lossless. u32 casts on capability slot indices and struct offsets are
// bounded by CSpace capacity and page size.
#![allow(clippy::cast_possible_truncation)]

use core::panic::PanicInfo;

use init_protocol::{CapDescriptor, CapType, InitInfo, INIT_PROTOCOL_VERSION};

mod arch;
mod bootstrap;
pub(crate) mod logging;
mod service;
mod vfs;

// ── Constants ────────────────────────────────────────────────────────────────

/// Page size (4 KiB).
pub(crate) const PAGE_SIZE: u64 = 0x1000;

/// Base virtual address for temporary mappings in init's address space.
/// Well above init's code/data/stack to avoid conflicts.
pub(crate) const TEMP_MAP_BASE: u64 = 0x0000_0001_0000_0000; // 4 GiB

/// Virtual address for init's own IPC buffer page (explicitly mapped).
const INIT_IPC_BUF_VA: u64 = 0x0000_0000_C000_0000; // 3 GiB

// ── Cap descriptor helpers ───────────────────────────────────────────────────

pub(crate) fn descriptors(info: &InitInfo) -> &[CapDescriptor]
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
pub(crate) fn find_cap_by_type(info: &InitInfo, wanted: CapType) -> Option<u32>
{
    descriptors(info)
        .iter()
        .find(|d| d.cap_type == wanted)
        .map(|d| d.slot)
}

// dead_code: used by the riscv64 serial module but not x86_64.
#[allow(dead_code)]
pub(crate) fn find_cap(info: &InitInfo, wanted_type: CapType, wanted_aux0: u64) -> Option<u32>
{
    descriptors(info)
        .iter()
        .find(|d| d.cap_type == wanted_type && d.aux0 == wanted_aux0)
        .map(|d| d.slot)
}

// ── Simple frame allocator ──────────────────────────────────────────────────

/// Bump allocator over init's memory pool frame caps.
///
/// Splits page-sized frames from the first available memory pool frame cap
/// using `frame_split`. When a frame is exhausted, moves to the next.
pub(crate) struct FrameAlloc
{
    /// Current frame cap being split (covers remaining unallocated region).
    current: u32,
    /// Remaining size in bytes of current frame.
    remaining: u64,
    /// Index into the memory frame range for the next frame to use.
    pub(crate) next_idx: u32,
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
    pub(crate) fn alloc_page(&mut self) -> Option<u32>
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
    pub(crate) fn alloc_zero_page(&mut self, aspace: u32, va: u64) -> Option<u32>
    {
        let cap = self.alloc_page()?;
        syscall::mem_map(cap, aspace, va, 0, 1, syscall::MAP_WRITABLE).ok()?;
        // SAFETY: va is mapped writable and covers one page.
        unsafe {
            core::ptr::write_bytes(va as *mut u8, 0, PAGE_SIZE as usize);
        }
        Some(cap)
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
    logging::log("init: starting");

    let mut alloc = FrameAlloc::new(info);

    // Map a fresh page for init's IPC buffer.
    let Some(ipc_cap) = alloc.alloc_page()
    else
    {
        logging::log("init: FATAL: cannot allocate IPC buffer frame");
        syscall::thread_exit();
    };
    if syscall::mem_map(
        ipc_cap,
        info.aspace_cap,
        INIT_IPC_BUF_VA,
        0,
        1,
        syscall::MAP_WRITABLE,
    )
    .is_err()
    {
        logging::log("init: FATAL: cannot map IPC buffer page");
        syscall::thread_exit();
    }
    // Zero the IPC buffer page.
    // SAFETY: INIT_IPC_BUF_VA is mapped writable, one page.
    unsafe { core::ptr::write_bytes(INIT_IPC_BUF_VA as *mut u8, 0, PAGE_SIZE as usize) };
    if syscall::ipc_buffer_set(INIT_IPC_BUF_VA).is_err()
    {
        logging::log("init: FATAL: ipc_buffer_set failed");
        syscall::thread_exit();
    }
    logging::log("init: IPC buffer registered");

    // ── Bootstrap procmgr ────────────────────────────────────────────────────

    let Some(endpoint_cap) = bootstrap::bootstrap_procmgr(info, &mut alloc)
    else
    {
        logging::log("init: FATAL: failed to bootstrap procmgr");
        syscall::thread_exit();
    };

    // ── Create log endpoint ────────────────────────────────────────────────────

    let Ok(log_ep) = syscall::cap_create_endpoint()
    else
    {
        logging::log("init: FATAL: cannot create log endpoint");
        syscall::thread_exit();
    };
    logging::log("init: log endpoint created");

    // ── Create inter-service endpoints ──────────────────────────────────────────

    let Ok(devmgr_registry_ep) = syscall::cap_create_endpoint()
    else
    {
        logging::log("init: FATAL: cannot create devmgr registry endpoint");
        syscall::thread_exit();
    };
    logging::log("init: devmgr registry endpoint created");

    let Ok(vfsd_service_ep) = syscall::cap_create_endpoint()
    else
    {
        logging::log("init: FATAL: cannot create vfsd service endpoint");
        syscall::thread_exit();
    };
    logging::log("init: vfsd service endpoint created");

    // ── Request procmgr to create early services ──────────────────────────────

    // SAFETY: INIT_IPC_BUF_VA is the registered IPC buffer, page-aligned.
    #[allow(clippy::cast_ptr_alignment)]
    let ipc_buf = INIT_IPC_BUF_VA as *mut u64;

    if info.module_frame_count >= 2
    {
        logging::log("init: requesting procmgr to create devmgr (with hw caps)");
        service::create_devmgr_with_caps(info, endpoint_cap, log_ep, devmgr_registry_ep, ipc_buf);
    }
    else
    {
        logging::log("init: no devmgr module available");
    }

    if info.module_frame_count >= 3
    {
        logging::log("init: requesting procmgr to create vfsd (with caps)");
        service::create_vfsd_with_caps(
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
        logging::log("init: no vfsd module available");
    }

    // Spawn log thread so services can log while main thread continues.
    let ioport_cap = find_cap_by_type(info, init_protocol::CapType::IoPortRange).unwrap_or(0);
    logging::spawn_log_thread(info, &mut alloc, log_ep, ioport_cap);

    // Switch main thread from direct serial to IPC-based logging through the
    // log thread. All subsequent log() calls go via IPC — clean, serialized.
    logging::set_ipc_logging(log_ep, ipc_buf);
    logging::log("init: log thread started");

    // Phase 1 bootstrap complete — services are running, log thread active.
    logging::log("init: phase 1 bootstrap complete");

    // ── Phase 2: mount root filesystem ──────────────────────────────────────

    // SAFETY: InitInfo page is valid and contains cmdline data.
    let cmdline = unsafe { init_protocol::cmdline_bytes(info) };
    logging::log("init: phase 2: parsing cmdline");

    let mut root_uuid = [0u8; 16];
    if !vfs::parse_root_uuid(cmdline, &mut root_uuid)
    {
        logging::log("init: FATAL: no root=UUID= in cmdline");
        syscall::thread_exit();
    }

    logging::log("init: phase 2: mounting root filesystem");
    if !vfs::send_mount(vfsd_service_ep, ipc_buf, &root_uuid, b"/")
    {
        logging::log("init: FATAL: root mount failed");
        syscall::thread_exit();
    }
    logging::log("init: phase 2: root mounted at /");

    // ── Phase 2b: read /config/mounts.conf, mount additional filesystems ────

    logging::log("init: phase 2: reading /config/mounts.conf");
    let mut conf_buf = [0u8; 512];
    let conf_len = vfs::vfs_read_file(
        vfsd_service_ep,
        ipc_buf,
        b"/config/mounts.conf",
        &mut conf_buf,
    );

    if conf_len > 0
    {
        logging::log("init: phase 2: processing mounts.conf");
        vfs::process_mounts_conf(&conf_buf[..conf_len], vfsd_service_ep, ipc_buf);
    }
    else
    {
        logging::log("init: phase 2: no mounts.conf or empty");
    }

    // End-to-end verification: read a file across the multi-mount namespace.
    logging::log("init: phase 2: verifying /esp/EFI/seraph/boot.conf");
    let mut verify_buf = [0u8; 512];
    let verify_len = vfs::vfs_read_file(
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
        logging::log(s);
    }
    else
    {
        logging::log("init: phase 2: boot.conf read FAILED");
    }

    logging::log("init: phase 2 bootstrap complete");

    // ── Phase 3: svcmgr, service registration, handover ────────────────────

    service::phase3_svcmgr_handover(info, endpoint_cap, log_ep, vfsd_service_ep, ipc_buf);
}

/// Idle loop fallback when Phase 3 cannot proceed.
pub(crate) fn idle_loop() -> !
{
    loop
    {
        let _ = syscall::thread_yield();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> !
{
    logging::log("init: PANIC");
    syscall::thread_exit();
}
