// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// drivers/virtio/blk/src/main.rs

//! Seraph `VirtIO` block device driver.
//!
//! Receives BAR MMIO cap, IRQ cap, and `VirtioPciStartupInfo` startup message
//! from devmgr. Initialises the `VirtIO` device via the modern PCI transport,
//! sets up a split virtqueue, and performs a test read of sector 0 to verify
//! end-to-end operation.

#![no_std]
#![no_main]
#![allow(clippy::cast_possible_truncation)]

extern crate runtime;

use process_abi::{CapType, StartupInfo};
use virtio_core::pci::PciTransport;
use virtio_core::virtqueue::{self, SplitVirtqueue};
use virtio_core::{
    VirtioPciStartupInfo, STATUS_ACKNOWLEDGE, STATUS_DRIVER, STATUS_DRIVER_OK, STATUS_FEATURES_OK,
};

// ── Constants ──────────────────────────────────────────────────────────────

const PAGE_SIZE: u64 = 0x1000;

/// VA for mapping BAR0 MMIO.
const BAR_MAP_VA: u64 = 0x0000_0001_0000_0000; // 4 GiB

/// VA for mapping virtqueue ring pages (DMA memory).
const RING_MAP_VA: u64 = 0x0000_0001_0001_0000;

/// VA for mapping the data buffer page (DMA memory).
const DATA_MAP_VA: u64 = 0x0000_0001_0010_0000;

/// IPC label for `REQUEST_FRAMES` (procmgr).
const LABEL_REQUEST_FRAMES: u64 = 5;

/// Queue size we request (must be <= device max).
const QUEUE_SIZE: u16 = 128;

/// Sentinel value in `CapDescriptor.aux0` indicating a log endpoint.
const LOG_ENDPOINT_SENTINEL: u64 = 0xFFFF_FFFF_FFFF_FFFF;

/// Sentinel value in `CapDescriptor.aux0` indicating a service endpoint.
const SERVICE_ENDPOINT_SENTINEL: u64 = 0xFFFF_FFFF_FFFF_FFFE;

use runtime::log::{log, log_hex};

// ── VirtIO block request header (VirtIO 1.2 §5.2.6) ───────────────────────

/// Block request type: read.
const VIRTIO_BLK_T_IN: u32 = 0;

/// Block request header.
#[repr(C)]
struct VirtioBlkReqHeader
{
    req_type: u32,
    reserved: u32,
    sector: u64,
}

// ── Driver caps from startup info ──────────────────────────────────────────

struct DriverCaps
{
    bar_mmio_slot: u32,
    irq_slot: u32,
    procmgr_ep: u32,
    log_ep: u32,
    service_ep: u32,
    self_aspace: u32,
}

fn classify_startup_caps(startup: &StartupInfo) -> DriverCaps
{
    let mut caps = DriverCaps {
        bar_mmio_slot: 0,
        irq_slot: 0,
        procmgr_ep: 0,
        log_ep: 0,
        service_ep: 0,
        self_aspace: startup.self_aspace,
    };

    for d in startup.initial_caps
    {
        match d.cap_type
        {
            CapType::MmioRegion if caps.bar_mmio_slot == 0 =>
            {
                caps.bar_mmio_slot = d.slot;
            }
            CapType::Interrupt =>
            {
                caps.irq_slot = d.slot;
            }
            CapType::Frame if d.aux0 == LOG_ENDPOINT_SENTINEL =>
            {
                caps.log_ep = d.slot;
            }
            CapType::Frame if d.aux0 == SERVICE_ENDPOINT_SENTINEL =>
            {
                caps.service_ep = d.slot;
            }
            // Sentinel: procmgr endpoint (Frame with aux0=0, aux1=0).
            CapType::Frame if d.aux0 == 0 && d.aux1 == 0 =>
            {
                caps.procmgr_ep = d.slot;
            }
            _ =>
            {}
        }
    }

    caps
}

// ── Frame allocation via procmgr IPC ───────────────────────────────────────

/// Request `page_count` frame caps from procmgr. Returns the first cap slot
/// on success.
fn request_frames(procmgr_ep: u32, page_count: u64, ipc_buf: *mut u64) -> Option<u32>
{
    // SAFETY: ipc_buf is the registered IPC buffer.
    unsafe { core::ptr::write_volatile(ipc_buf, page_count) };

    let Ok((label, _)) = syscall::ipc_call(procmgr_ep, LABEL_REQUEST_FRAMES, 1, &[])
    else
    {
        return None;
    };
    if label != 0
    {
        return None;
    }

    // SAFETY: ipc_buf is registered IPC buffer.
    #[allow(clippy::cast_ptr_alignment)]
    let (cap_count, cap_slots) = unsafe { syscall::read_recv_caps(ipc_buf) };
    if cap_count == 0
    {
        return None;
    }
    Some(cap_slots[0])
}

// ── Entry point ────────────────────────────────────────────────────────────

#[no_mangle]
#[allow(clippy::too_many_lines)]
extern "Rust" fn main(startup: &StartupInfo) -> !
{
    // Register IPC buffer (must be first — needed for IPC logging).
    if syscall::ipc_buffer_set(startup.ipc_buffer as u64).is_err()
    {
        syscall::thread_exit();
    }
    // cast_ptr_alignment: IPC buffer is page-aligned (4096-byte), satisfying u64 alignment.
    #[allow(clippy::cast_ptr_alignment)]
    let ipc_buf = startup.ipc_buffer.cast::<u64>();

    // Parse startup caps.
    let caps = classify_startup_caps(startup);

    // Initialise IPC logging.
    if caps.log_ep != 0
    {
        // SAFETY: single-threaded; called once before any log calls.
        unsafe { runtime::log::log_init(caps.log_ep, startup.ipc_buffer) };
    }

    log("virtio-blk: starting");
    if caps.bar_mmio_slot == 0
    {
        log("virtio-blk: no BAR MMIO cap");
        syscall::thread_exit();
    }
    if caps.procmgr_ep == 0
    {
        log("virtio-blk: no procmgr endpoint");
        syscall::thread_exit();
    }

    // Parse VirtIO PCI startup info from startup message.
    let Some(pci_info) = VirtioPciStartupInfo::from_bytes(startup.startup_message)
    else
    {
        log("virtio-blk: no VirtIO PCI startup info");
        syscall::thread_exit();
    };

    // Map BAR MMIO.
    if syscall::mmio_map(caps.self_aspace, caps.bar_mmio_slot, BAR_MAP_VA, 0).is_err()
    {
        log("virtio-blk: BAR mmio_map failed");
        syscall::thread_exit();
    }

    // Create PCI transport.
    let transport = PciTransport::new(BAR_MAP_VA, &pci_info);

    // ── Device initialisation (VirtIO 1.2 §3.1.1) ─────────────────────

    // 1. Reset.
    transport.reset();

    // 2. ACKNOWLEDGE.
    transport.set_status(STATUS_ACKNOWLEDGE);

    // 3. DRIVER.
    transport.set_status(STATUS_ACKNOWLEDGE | STATUS_DRIVER);

    let features = transport.negotiate_features(|device_features| {
        // Accept only VIRTIO_F_VERSION_1 (bit 32) — required for modern devices.
        device_features & (1 << 32)
    });
    if features.is_none()
    {
        log("virtio-blk: feature negotiation failed");
        syscall::thread_exit();
    }

    // Read device capacity (sectors) from device config.
    let capacity = transport.config_read_u64(0);
    log_hex("virtio-blk: capacity (sectors)=", capacity);

    // 5. Setup virtqueue 0 (requestq).
    transport.queue_select(0);
    let max_size = transport.queue_max_size();
    let queue_size = QUEUE_SIZE.min(max_size);
    transport.queue_set_size(queue_size);

    // Allocate DMA memory for virtqueue rings.
    let ring_pages = virtqueue::ring_pages(queue_size) as u64;
    let Some(ring_frame) = request_frames(caps.procmgr_ep, ring_pages, ipc_buf)
    else
    {
        log("virtio-blk: failed to allocate ring frames");
        syscall::thread_exit();
    };

    // Map ring pages.
    if syscall::mem_map(
        ring_frame,
        caps.self_aspace,
        RING_MAP_VA,
        0,
        ring_pages,
        syscall::PROT_READ | syscall::PROT_WRITE,
    )
    .is_err()
    {
        log("virtio-blk: ring mem_map failed");
        syscall::thread_exit();
    }

    // Zero the ring memory.
    // SAFETY: RING_MAP_VA is mapped writable, ring_pages * PAGE_SIZE bytes.
    unsafe {
        core::ptr::write_bytes(RING_MAP_VA as *mut u8, 0, (ring_pages * PAGE_SIZE) as usize);
    }

    // Get physical addresses for device programming.
    let Ok(ring_phys) = syscall::dma_grant(ring_frame, 0, syscall_abi::FLAG_DMA_UNSAFE)
    else
    {
        log("virtio-blk: ring dma_grant failed");
        syscall::thread_exit();
    };

    // Layout: descriptors | avail ring | used ring, contiguous in physical memory.
    let desc_size = virtqueue::desc_table_size(queue_size);
    let avail_size = virtqueue::avail_ring_size(queue_size);

    let desc_phys = ring_phys;
    let avail_phys = ring_phys + desc_size as u64;
    let used_phys = avail_phys + avail_size as u64;

    let desc_va = RING_MAP_VA;
    let avail_va = RING_MAP_VA + desc_size as u64;
    let used_va = avail_va + avail_size as u64;

    // Program queue addresses.
    transport.queue_set_desc_lo(desc_phys as u32);
    transport.queue_set_desc_hi((desc_phys >> 32) as u32);
    transport.queue_set_avail_lo(avail_phys as u32);
    transport.queue_set_avail_hi((avail_phys >> 32) as u32);
    transport.queue_set_used_lo(used_phys as u32);
    transport.queue_set_used_hi((used_phys >> 32) as u32);

    // Enable queue.
    transport.queue_set_ready(1);

    // Save notification offset for this queue before changing selection.
    let queue_notify_off = transport.queue_notify_off();

    // 6. DRIVER_OK.
    transport
        .set_status(STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK);
    log("virtio-blk: device ready");

    // Create virtqueue manager.
    // SAFETY: ring memory is zeroed, properly sized, and exclusively owned.
    // Pointers are aligned: desc_va is page-aligned; avail_va is at desc_va +
    // queue_size*16 (always 2-byte aligned); used_va is at avail_va + 4 + 2*queue_size
    // (always 4-byte aligned for VirtqUsedElem).
    let mut vq = unsafe {
        SplitVirtqueue::new(
            desc_va as *mut virtqueue::VirtqDesc,
            avail_va as *mut virtqueue::VirtqAvail,
            used_va as *mut virtqueue::VirtqUsed,
            queue_size,
        )
    };

    // ── Test read: sector 0 ────────────────────────────────────────────

    // Allocate a data buffer page for the block read.
    let Some(data_frame) = request_frames(caps.procmgr_ep, 1, ipc_buf)
    else
    {
        log("virtio-blk: failed to allocate data frame");
        syscall::thread_exit();
    };

    if syscall::mem_map(
        data_frame,
        caps.self_aspace,
        DATA_MAP_VA,
        0,
        1,
        syscall::PROT_READ | syscall::PROT_WRITE,
    )
    .is_err()
    {
        log("virtio-blk: data mem_map failed");
        syscall::thread_exit();
    }

    // Fill data buffer with sentinel pattern (0xAA) to detect untouched regions.
    // SAFETY: DATA_MAP_VA is mapped writable, one page.
    unsafe { core::ptr::write_bytes(DATA_MAP_VA as *mut u8, 0xAA, PAGE_SIZE as usize) };

    let Ok(data_phys) = syscall::dma_grant(data_frame, 0, syscall_abi::FLAG_DMA_UNSAFE)
    else
    {
        log("virtio-blk: data dma_grant failed");
        syscall::thread_exit();
    };

    let header_va = DATA_MAP_VA as *mut VirtioBlkReqHeader;
    let data_buf_va = DATA_MAP_VA + 512;
    let status_va = DATA_MAP_VA + 1024;

    let header_phys = data_phys;
    let data_buf_phys = data_phys + 512;
    let status_phys = data_phys + 1024;

    // SAFETY: header_va is within the mapped data page, properly aligned.
    unsafe {
        (*header_va).req_type = VIRTIO_BLK_T_IN;
        (*header_va).reserved = 0;
        (*header_va).sector = 0;
    }
    // SAFETY: status_va is within the mapped data page.
    unsafe { core::ptr::write_volatile(status_va as *mut u8, 0xFF) };

    // Submit descriptor chain: header (readable), data (writable), status (writable).
    let chain = [
        (header_phys, 16, false),   // request header
        (data_buf_phys, 512, true), // data buffer (device writes)
        (status_phys, 1, true),     // status byte (device writes)
    ];

    let Some(_head) = vq.add_chain(&chain)
    else
    {
        log("virtio-blk: failed to submit read request");
        syscall::thread_exit();
    };

    // Notify device.
    transport.notify(0, queue_notify_off);

    log("virtio-blk: sector 0 read submitted, polling...");

    // Poll for completion (no IRQ setup yet — simple busy-wait).
    let mut attempts = 0u32;
    loop
    {
        if let Some((_idx, _len)) = vq.poll_used()
        {
            break;
        }
        attempts += 1;
        if attempts > 10_000_000
        {
            log("virtio-blk: read timed out");
            syscall::thread_exit();
        }
        core::hint::spin_loop();
    }

    // Check status byte.
    // SAFETY: status_va is within the mapped data page.
    let status = unsafe { core::ptr::read_volatile(status_va as *const u8) };
    if status != 0
    {
        log_hex("virtio-blk: read failed, status=", u64::from(status));
        syscall::thread_exit();
    }

    log("virtio-blk: sector 0 read OK");

    // ── Service endpoint receive loop ─────────────────────────────────

    if caps.service_ep == 0
    {
        log("virtio-blk: no service endpoint, entering idle loop");
        loop
        {
            let _ = syscall::thread_yield();
        }
    }

    log("virtio-blk: ready, entering service loop");
    loop
    {
        let Ok((label, _)) = syscall::ipc_recv(caps.service_ep)
        else
        {
            continue;
        };

        match label
        {
            // READ_BLOCK: data[0] = sector number.
            // Reads a single 512-byte sector and replies with data in the
            // first 64 IPC buffer words (512 bytes). Label 0 = success.
            1 =>
            {
                // SAFETY: IPC buffer is valid; kernel wrote request data.
                let sector = unsafe { core::ptr::read_volatile(ipc_buf) };

                // Write request header.
                // SAFETY: header_va is within the mapped data page.
                unsafe {
                    (*header_va).req_type = VIRTIO_BLK_T_IN;
                    (*header_va).reserved = 0;
                    (*header_va).sector = sector;
                }
                // SAFETY: status_va is within the mapped data page.
                unsafe { core::ptr::write_volatile(status_va as *mut u8, 0xFF) };

                let Some(_head) = vq.add_chain(&chain)
                else
                {
                    let _ = syscall::ipc_reply(0xFF, 0, &[]);
                    continue;
                };
                transport.notify(0, queue_notify_off);

                // Poll for completion.
                let mut timed_out = false;
                let mut attempts = 0u32;
                loop
                {
                    if vq.poll_used().is_some()
                    {
                        break;
                    }
                    attempts += 1;
                    if attempts > 10_000_000
                    {
                        timed_out = true;
                        break;
                    }
                    core::hint::spin_loop();
                }

                if timed_out
                {
                    let _ = syscall::ipc_reply(0xFE, 0, &[]);
                    continue;
                }

                // Check device status byte.
                // SAFETY: status_va is within the mapped data page.
                let dev_status = unsafe { core::ptr::read_volatile(status_va as *const u8) };
                if dev_status != 0
                {
                    let _ = syscall::ipc_reply(u64::from(dev_status), 0, &[]);
                    continue;
                }

                // Copy sector data (512 bytes = 64 words) into IPC buffer for reply.
                for i in 0..64u64
                {
                    // SAFETY: data_buf_va + i*8 is within the mapped page;
                    // ipc_buf + i is within the IPC buffer page.
                    unsafe {
                        let word = core::ptr::read_volatile((data_buf_va + i * 8) as *const u64);
                        core::ptr::write_volatile(ipc_buf.add(i as usize), word);
                    }
                }

                let _ = syscall::ipc_reply(0, 64, &[]);
            }
            _ =>
            {
                let _ = syscall::ipc_reply(0xFF, 0, &[]);
            }
        }
    }
}
