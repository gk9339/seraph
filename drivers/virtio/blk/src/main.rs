// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// drivers/virtio/blk/src/main.rs

//! Seraph `VirtIO` block device driver.
//!
//! Receives BAR MMIO cap, IRQ cap, and `VirtioPciStartupInfo` startup message
//! from devmgr. Initialises the `VirtIO` device via the modern PCI transport,
//! sets up a split virtqueue, and serves block read requests over IPC.

#![no_std]
#![no_main]
// cast_possible_truncation: userspace targets 64-bit only; u64/usize conversions
// are lossless. u32 casts on capability slot indices are bounded by CSpace capacity.
#![allow(clippy::cast_possible_truncation)]

extern crate runtime;

mod io;

use ipc::{blk_labels, procmgr_labels, LOG_ENDPOINT_SENTINEL, SERVICE_ENDPOINT_SENTINEL};
use process_abi::{CapType, StartupInfo};
use virtio_core::pci::PciTransport;
use virtio_core::virtqueue::{self, SplitVirtqueue};
use virtio_core::{
    VirtioPciStartupInfo, STATUS_ACKNOWLEDGE, STATUS_DRIVER, STATUS_DRIVER_OK, STATUS_FEATURES_OK,
};

use crate::io::IoLayout;

// ── Constants ──────────────────────────────────────────────────────────────

const PAGE_SIZE: u64 = 0x1000;

/// VA for mapping BAR0 MMIO.
const BAR_MAP_VA: u64 = 0x0000_0001_0000_0000; // 4 GiB

/// VA for mapping virtqueue ring pages (DMA memory).
const RING_MAP_VA: u64 = 0x0000_0001_0001_0000;

/// VA for mapping the data buffer page (DMA memory).
const DATA_MAP_VA: u64 = 0x0000_0001_0010_0000;

/// Queue size we request (must be <= device max).
const QUEUE_SIZE: u16 = 128;

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

    let Ok((label, _)) = syscall::ipc_call(procmgr_ep, procmgr_labels::REQUEST_FRAMES, 1, &[])
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

// ── Device initialisation ──────────────────────────────────────────────────

/// Initialise the `VirtIO` device through the standard sequence (`VirtIO` 1.2
/// section 3.1.1): reset, acknowledge, negotiate features, read capacity.
fn init_device(transport: &PciTransport) -> u64
{
    transport.reset();
    transport.set_status(STATUS_ACKNOWLEDGE);
    transport.set_status(STATUS_ACKNOWLEDGE | STATUS_DRIVER);

    let features = transport.negotiate_features(|device_features| {
        // Accept only VIRTIO_F_VERSION_1 (bit 32) — required for modern devices.
        device_features & (1 << 32)
    });
    if features.is_none()
    {
        runtime::log!("virtio-blk: feature negotiation failed");
        syscall::thread_exit();
    }

    transport.config_read_u64(0)
}

/// Set up virtqueue 0 (requestq): allocate ring DMA memory, map it, program
/// the device, and return a `SplitVirtqueue` + notification offset.
///
/// # Panics
///
/// Exits the thread on allocation or mapping failure (no recovery path).
#[allow(clippy::too_many_lines)]
fn setup_virtqueue(
    transport: &PciTransport,
    caps: &DriverCaps,
    ipc_buf: *mut u64,
) -> (SplitVirtqueue, u16)
{
    transport.queue_select(0);
    let max_size = transport.queue_max_size();
    let queue_size = QUEUE_SIZE.min(max_size);
    transport.queue_set_size(queue_size);

    // Allocate DMA memory for virtqueue rings.
    let ring_pages = virtqueue::ring_pages(queue_size) as u64;
    let Some(ring_frame) = request_frames(caps.procmgr_ep, ring_pages, ipc_buf)
    else
    {
        runtime::log!("virtio-blk: failed to allocate ring frames");
        syscall::thread_exit();
    };

    // Map ring pages.
    if syscall::mem_map(
        ring_frame,
        caps.self_aspace,
        RING_MAP_VA,
        0,
        ring_pages,
        syscall::MAP_READONLY | syscall::MAP_WRITABLE,
    )
    .is_err()
    {
        runtime::log!("virtio-blk: ring mem_map failed");
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
        runtime::log!("virtio-blk: ring dma_grant failed");
        syscall::thread_exit();
    };

    // Layout: descriptors | avail ring | [pad] | used ring (4-byte aligned).
    let desc_size = virtqueue::desc_table_size(queue_size);
    let used_off = virtqueue::used_ring_offset(queue_size);

    let desc_phys = ring_phys;
    let avail_phys = ring_phys + desc_size as u64;
    let used_phys = ring_phys + used_off as u64;

    let desc_va = RING_MAP_VA;
    let avail_va = RING_MAP_VA + desc_size as u64;
    let used_va = RING_MAP_VA + used_off as u64;

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

    // Create virtqueue manager.
    // SAFETY: ring memory is zeroed, properly sized, and exclusively owned.
    // Pointers are aligned: desc_va is page-aligned; avail_va is at desc_va +
    // queue_size*16 (always 2-byte aligned); used_va is at avail_va + 4 + 2*queue_size
    // (always 4-byte aligned for VirtqUsedElem).
    let vq = unsafe {
        SplitVirtqueue::new(
            desc_va as *mut virtqueue::VirtqDesc,
            avail_va as *mut virtqueue::VirtqAvail,
            used_va as *mut virtqueue::VirtqUsed,
            queue_size,
        )
    };

    (vq, queue_notify_off)
}

/// Allocate and map the data buffer page for block I/O, returning an `IoLayout`.
fn setup_io_buffer(caps: &DriverCaps, ipc_buf: *mut u64) -> IoLayout
{
    let Some(data_frame) = request_frames(caps.procmgr_ep, 1, ipc_buf)
    else
    {
        runtime::log!("virtio-blk: failed to allocate data frame");
        syscall::thread_exit();
    };

    if syscall::mem_map(
        data_frame,
        caps.self_aspace,
        DATA_MAP_VA,
        0,
        1,
        syscall::MAP_READONLY | syscall::MAP_WRITABLE,
    )
    .is_err()
    {
        runtime::log!("virtio-blk: data mem_map failed");
        syscall::thread_exit();
    }

    // Fill data buffer with sentinel pattern (0xAA) to detect untouched regions.
    // SAFETY: DATA_MAP_VA is mapped writable, one page.
    unsafe { core::ptr::write_bytes(DATA_MAP_VA as *mut u8, 0xAA, PAGE_SIZE as usize) };

    let Ok(data_phys) = syscall::dma_grant(data_frame, 0, syscall_abi::FLAG_DMA_UNSAFE)
    else
    {
        runtime::log!("virtio-blk: data dma_grant failed");
        syscall::thread_exit();
    };

    IoLayout {
        data_va: DATA_MAP_VA,
        data_phys,
    }
}

// ── Service loop ───────────────────────────────────────────────────────────

/// Handle incoming IPC requests on the service endpoint.
#[allow(clippy::too_many_arguments)]
fn service_loop(
    service_ep: u32,
    layout: &IoLayout,
    vq: &mut SplitVirtqueue,
    transport: &PciTransport,
    queue_notify_off: u16,
    irq_signal: u32,
    irq_cap: u32,
    ipc_buf: *mut u64,
) -> !
{
    runtime::log!("virtio-blk: ready, entering service loop");
    loop
    {
        let Ok((label, _token)) = syscall::ipc_recv(service_ep)
        else
        {
            continue;
        };

        match label
        {
            blk_labels::READ_BLOCK =>
            {
                handle_read_block(
                    layout,
                    vq,
                    transport,
                    queue_notify_off,
                    irq_signal,
                    irq_cap,
                    ipc_buf,
                );
            }
            _ =>
            {
                let _ = syscall::ipc_reply(0xFF, 0, &[]);
            }
        }
    }
}

/// Handle a `READ_BLOCK` request: read data[0] as the sector number, perform the
/// I/O via IRQ-driven completion, and reply with 512 bytes (64 IPC words) on success.
#[allow(clippy::too_many_arguments)]
fn handle_read_block(
    layout: &IoLayout,
    vq: &mut SplitVirtqueue,
    transport: &PciTransport,
    queue_notify_off: u16,
    irq_signal: u32,
    irq_cap: u32,
    ipc_buf: *mut u64,
)
{
    // SAFETY: IPC buffer is valid; kernel wrote request data.
    let sector = unsafe { core::ptr::read_volatile(ipc_buf) };

    if !io::submit_and_wait(
        layout,
        sector,
        vq,
        transport,
        queue_notify_off,
        irq_signal,
        irq_cap,
    )
    {
        let status = layout.read_status();
        let code = u64::from(status);
        let _ = syscall::ipc_reply(code, 0, &[]);
        return;
    }

    // SAFETY: ipc_buf is the registered IPC buffer with at least 64 writable words.
    unsafe { layout.copy_sector_to_ipc(ipc_buf) };
    let _ = syscall::ipc_reply(0, 64, &[]);
}

// ── Entry point ────────────────────────────────────────────────────────────

#[no_mangle]
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

    runtime::log!("virtio-blk: starting");
    if caps.bar_mmio_slot == 0
    {
        runtime::log!("virtio-blk: no BAR MMIO cap");
        syscall::thread_exit();
    }
    if caps.procmgr_ep == 0
    {
        runtime::log!("virtio-blk: no procmgr endpoint");
        syscall::thread_exit();
    }

    // Parse VirtIO PCI startup info from startup message.
    let Some(pci_info) = VirtioPciStartupInfo::from_bytes(startup.startup_message)
    else
    {
        runtime::log!("virtio-blk: no VirtIO PCI startup info");
        syscall::thread_exit();
    };

    // Map BAR MMIO.
    if syscall::mmio_map(caps.self_aspace, caps.bar_mmio_slot, BAR_MAP_VA, 0).is_err()
    {
        runtime::log!("virtio-blk: BAR mmio_map failed");
        syscall::thread_exit();
    }

    // Create PCI transport and initialise device.
    let transport = PciTransport::new(BAR_MAP_VA, &pci_info);
    let capacity = init_device(&transport);
    runtime::log!("virtio-blk: capacity (sectors)={:#018x}", capacity);

    // Set up virtqueue and data buffer.
    let (mut vq, queue_notify_off) = setup_virtqueue(&transport, &caps, ipc_buf);

    // DRIVER_OK.
    transport
        .set_status(STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK);
    runtime::log!("virtio-blk: device ready");

    // Set up IRQ-driven completion: create a signal and bind it to the IRQ.
    if caps.irq_slot == 0
    {
        runtime::log!("virtio-blk: no IRQ cap, cannot operate");
        syscall::thread_exit();
    }
    let Ok(irq_signal) = syscall::cap_create_signal()
    else
    {
        runtime::log!("virtio-blk: failed to create IRQ signal");
        syscall::thread_exit();
    };
    if syscall::irq_register(caps.irq_slot, irq_signal).is_err()
    {
        runtime::log!("virtio-blk: irq_register failed");
        syscall::thread_exit();
    }
    // Unmask the interrupt at the controller (IOAPIC/PLIC).
    // irq_register leaves the entry masked; the first irq_ack unmasks it.
    let _ = syscall::irq_ack(caps.irq_slot);
    let irq_cap = caps.irq_slot;

    // Set up I/O buffer and test-read sector 0.
    let layout = setup_io_buffer(&caps, ipc_buf);

    if !io::submit_and_wait(
        &layout,
        0,
        &mut vq,
        &transport,
        queue_notify_off,
        irq_signal,
        irq_cap,
    )
    {
        runtime::log!("virtio-blk: sector 0 test read failed");
        syscall::thread_exit();
    }
    runtime::log!("virtio-blk: sector 0 read OK");

    // Enter service loop.
    if caps.service_ep == 0
    {
        runtime::log!("virtio-blk: no service endpoint, entering idle loop");
        loop
        {
            let _ = syscall::thread_yield();
        }
    }

    service_loop(
        caps.service_ep,
        &layout,
        &mut vq,
        &transport,
        queue_notify_off,
        irq_signal,
        irq_cap,
        ipc_buf,
    );
}
