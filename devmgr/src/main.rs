// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// devmgr/src/main.rs

//! Seraph device manager — platform enumeration, hardware discovery, and
//! driver binding.
//!
//! devmgr receives hardware capabilities from init (MMIO regions, interrupt
//! lines, I/O port ranges, PCI ECAM windows, scheduling control) and uses
//! them to discover devices and spawn driver processes.
//!
//! See `devmgr/README.md` for the full design and `devmgr/docs/pci-enumeration.md`
//! for PCI enumeration details.

#![no_std]
#![no_main]
#![allow(clippy::cast_possible_truncation)]

extern crate runtime;

use process_abi::{CapDescriptor, CapType, ProcessInfo, StartupInfo};
use virtio_core::{
    VirtioCapLocation, VirtioPciStartupInfo, VIRTIO_PCI_CAP_COMMON_CFG, VIRTIO_PCI_CAP_DEVICE_CFG,
    VIRTIO_PCI_CAP_ISR_CFG, VIRTIO_PCI_CAP_NOTIFY_CFG,
};

// ── Constants ────────────────────────────────────────────────────────────────

const PAGE_SIZE: u64 = 0x1000;

/// IPC label for `CREATE_PROCESS` (procmgr).
const LABEL_CREATE_PROCESS: u64 = 1;

/// IPC label for `START_PROCESS` (procmgr).
const LABEL_START_PROCESS: u64 = 2;

/// IPC label for `REQUEST_FRAMES` (procmgr).
#[allow(dead_code)]
const LABEL_REQUEST_FRAMES: u64 = 5;

/// VA base for mapping ECAM (x86-64) or `VirtIO` MMIO regions (RISC-V).
const MMIO_MAP_VA: u64 = 0x0000_0001_0000_0000; // 4 GiB

/// VA for mapping driver `ProcessInfo` frames during cap injection.
const DRIVER_PI_VA: u64 = 0x0000_0002_0000_0000; // 8 GiB

/// Maximum discovered PCI devices.
const MAX_DEVICES: usize = 8;

/// Maximum BAR entries per device.
const MAX_BARS: usize = 6;

/// Maximum cap descriptors for driver delegation.
const MAX_DRIVER_DESCS: usize = 16;

/// Sentinel value in `CapDescriptor.aux0` indicating a log endpoint.
const LOG_ENDPOINT_SENTINEL: u64 = 0xFFFF_FFFF_FFFF_FFFF;

/// Sentinel value in `CapDescriptor.aux0` indicating a service endpoint.
const SERVICE_ENDPOINT_SENTINEL: u64 = 0xFFFF_FFFF_FFFF_FFFE;

/// Sentinel value in `CapDescriptor.aux0` indicating a registry endpoint.
const REGISTRY_ENDPOINT_SENTINEL: u64 = 0xFFFF_FFFF_FFFF_FFFD;

/// IPC label for `QUERY_BLOCK_DEVICE` (devmgr registry).
const LABEL_QUERY_BLOCK_DEVICE: u64 = 1;

use runtime::log::{log, log_hex};

// ── Cap classification ──────────────────────────────────────────────────────

struct PciMmioWindow
{
    slot: u32,
    base: u64,
    size: u64,
}

struct DevmgrCaps
{
    ecam_slot: u32,
    ecam_base: u64,
    ecam_size: u64,
    /// PCI MMIO windows (32-bit and optionally 64-bit).
    pci_mmio_windows: [PciMmioWindow; 2],
    pci_mmio_window_count: usize,
    irq_slots: [(u32, u32); 64], // (slot, irq_id)
    irq_count: usize,
    procmgr_ep: u32,
    log_ep: u32,
    registry_ep: u32,
    self_aspace: u32,
    self_thread: u32,
    driver_module_slots: [u32; 8],
    driver_module_count: usize,
}

impl DevmgrCaps
{
    fn new() -> Self
    {
        Self {
            ecam_slot: 0,
            ecam_base: 0,
            ecam_size: 0,
            pci_mmio_windows: [
                PciMmioWindow {
                    slot: 0,
                    base: 0,
                    size: 0,
                },
                PciMmioWindow {
                    slot: 0,
                    base: 0,
                    size: 0,
                },
            ],
            pci_mmio_window_count: 0,
            irq_slots: [(0, 0); 64],
            irq_count: 0,
            procmgr_ep: 0,
            log_ep: 0,
            registry_ep: 0,
            self_aspace: 0,
            self_thread: 0,
            driver_module_slots: [0; 8],
            driver_module_count: 0,
        }
    }
}

fn classify_caps(startup: &StartupInfo, caps: &mut DevmgrCaps)
{
    caps.self_aspace = startup.self_aspace;
    caps.self_thread = startup.self_thread;

    let mut hw_caps_seen = false;
    let mut hw_caps_done = false;
    let mut procmgr_ep_found = false;

    for d in startup.initial_caps
    {
        match d.cap_type
        {
            CapType::PciEcam =>
            {
                hw_caps_seen = true;
                caps.ecam_slot = d.slot;
                caps.ecam_base = d.aux0;
                caps.ecam_size = d.aux1;
            }
            CapType::MmioRegion =>
            {
                hw_caps_seen = true;
                // PCI MMIO windows are large MmioRegion caps (>= 256 MiB).
                // Other MmioRegion caps are platform devices (UART, IOAPIC, etc.)
                // that devmgr doesn't directly use.
                if d.aux1 >= 0x1000_0000 && caps.pci_mmio_window_count < 2
                {
                    let idx = caps.pci_mmio_window_count;
                    caps.pci_mmio_windows[idx] = PciMmioWindow {
                        slot: d.slot,
                        base: d.aux0,
                        size: d.aux1,
                    };
                    caps.pci_mmio_window_count += 1;
                }
            }
            CapType::Interrupt =>
            {
                hw_caps_seen = true;
                if caps.irq_count < 64
                {
                    caps.irq_slots[caps.irq_count] = (d.slot, d.aux0 as u32);
                    caps.irq_count += 1;
                }
            }
            CapType::IoPortRange | CapType::SchedControl =>
            {
                hw_caps_seen = true;
            }
            CapType::Frame =>
            {
                if d.aux0 == LOG_ENDPOINT_SENTINEL
                {
                    // Log endpoint sentinel.
                    caps.log_ep = d.slot;
                }
                else if d.aux0 == REGISTRY_ENDPOINT_SENTINEL
                {
                    // Device registry endpoint (injected by init).
                    caps.registry_ep = d.slot;
                }
                else if hw_caps_seen && !hw_caps_done && !procmgr_ep_found
                {
                    // First Frame cap after hw caps with aux0=0: procmgr endpoint.
                    if d.aux0 == 0 && d.aux1 == 0
                    {
                        caps.procmgr_ep = d.slot;
                        procmgr_ep_found = true;
                        hw_caps_done = true;
                    }
                }
                else if procmgr_ep_found && caps.driver_module_count < 8
                {
                    // Driver module frame caps follow the endpoint.
                    caps.driver_module_slots[caps.driver_module_count] = d.slot;
                    caps.driver_module_count += 1;
                }
            }
            CapType::SbiControl =>
            {}
        }
    }
}

// ── PCI device discovery (x86-64) ───────────────────────────────────────────

#[derive(Clone, Copy)]
struct PciDevice
{
    bus: u8,
    dev: u8,
    func: u8,
    vendor_id: u16,
    device_id: u16,
    bar_phys: [u64; MAX_BARS],
    bar_size: [u64; MAX_BARS],
    bar_is_mmio: [bool; MAX_BARS],
    /// PCI BAR register index (0-5) for each discovered BAR.
    bar_pci_idx: [u8; MAX_BARS],
    bar_count: usize,
    irq_line: u8,
    /// `VirtIO` PCI startup info (populated for `VirtIO` devices only).
    virtio_info: VirtioPciStartupInfo,
}

/// Read a u16 from ECAM config space.
///
/// # Safety
/// `ecam_va` must be a valid MMIO mapping of the ECAM region, and
/// `offset` must be within the mapped region.
unsafe fn ecam_read16(ecam_va: u64, bus: u8, dev: u8, func: u8, reg: u16) -> u16
{
    let off =
        (u64::from(bus) << 20) | (u64::from(dev) << 15) | (u64::from(func) << 12) | u64::from(reg);
    // SAFETY: caller guarantees ecam_va + off is within the MMIO mapping.
    unsafe { core::ptr::read_volatile((ecam_va + off) as *const u16) }
}

/// Read a u32 from ECAM config space.
///
/// # Safety
/// See [`ecam_read16`].
unsafe fn ecam_read32(ecam_va: u64, bus: u8, dev: u8, func: u8, reg: u16) -> u32
{
    let off =
        (u64::from(bus) << 20) | (u64::from(dev) << 15) | (u64::from(func) << 12) | u64::from(reg);
    // SAFETY: caller guarantees ecam_va + off is within the MMIO mapping.
    unsafe { core::ptr::read_volatile((ecam_va + off) as *const u32) }
}

/// Write a u32 to ECAM config space.
///
/// # Safety
/// See [`ecam_read16`].
unsafe fn ecam_write32(ecam_va: u64, bus: u8, dev: u8, func: u8, reg: u16, val: u32)
{
    let off =
        (u64::from(bus) << 20) | (u64::from(dev) << 15) | (u64::from(func) << 12) | u64::from(reg);
    // SAFETY: caller guarantees ecam_va + off is within the MMIO mapping.
    unsafe { core::ptr::write_volatile((ecam_va + off) as *mut u32, val) }
}

/// Read a u8 from ECAM config space.
///
/// # Safety
/// See [`ecam_read16`].
unsafe fn ecam_read8(ecam_va: u64, bus: u8, dev: u8, func: u8, reg: u16) -> u8
{
    let off =
        (u64::from(bus) << 20) | (u64::from(dev) << 15) | (u64::from(func) << 12) | u64::from(reg);
    // SAFETY: caller guarantees ecam_va + off is within the MMIO mapping.
    unsafe { core::ptr::read_volatile((ecam_va + off) as *const u8) }
}

/// Scan PCI bus via ECAM and discover devices.
///
/// # Safety
/// `ecam_va` must be a valid ECAM MMIO mapping.
#[allow(clippy::too_many_lines)]
unsafe fn pci_enumerate(
    ecam_va: u64,
    start_bus: u8,
    end_bus: u8,
    devices: &mut [PciDevice; MAX_DEVICES],
) -> usize
{
    let mut count = 0;

    let mut bus = start_bus;
    while bus <= end_bus && count < MAX_DEVICES
    {
        for dev in 0..32u8
        {
            // SAFETY: ecam_va is a valid ECAM mapping (caller guarantee).
            let vendor = unsafe { ecam_read16(ecam_va, bus, dev, 0, 0x00) };
            if vendor == 0xFFFF
            {
                continue;
            }

            // SAFETY: ecam_va is valid.
            let header_type = unsafe { ecam_read8(ecam_va, bus, dev, 0, 0x0E) };
            let max_func = if header_type & 0x80 != 0 { 8 } else { 1 };

            for func in 0..max_func
            {
                if func > 0
                {
                    // SAFETY: ecam_va is valid.
                    let v = unsafe { ecam_read16(ecam_va, bus, dev, func, 0x00) };
                    if v == 0xFFFF
                    {
                        continue;
                    }
                }

                if count >= MAX_DEVICES
                {
                    break;
                }

                // SAFETY: ecam_va is valid ECAM mapping (caller guarantee).
                let device_id = unsafe { ecam_read16(ecam_va, bus, dev, func, 0x02) };
                // SAFETY: ecam_va is valid ECAM mapping.
                let irq_line = unsafe { ecam_read8(ecam_va, bus, dev, func, 0x3C) };
                // SAFETY: ecam_va is valid ECAM mapping.
                let ht = unsafe { ecam_read8(ecam_va, bus, dev, func, 0x0E) } & 0x7F;

                let mut pci_dev = PciDevice {
                    bus,
                    dev,
                    func,
                    vendor_id: vendor,
                    device_id,
                    bar_phys: [0; MAX_BARS],
                    bar_size: [0; MAX_BARS],
                    bar_is_mmio: [false; MAX_BARS],
                    bar_pci_idx: [0; MAX_BARS],
                    bar_count: 0,
                    irq_line,
                    virtio_info: VirtioPciStartupInfo::default(),
                };

                // Discover BARs (type-0 headers only: 6 BARs at 0x10-0x24).
                if ht == 0
                {
                    let mut bar_idx: u16 = 0;
                    while bar_idx < 6
                    {
                        let reg = 0x10 + bar_idx * 4;
                        // SAFETY: ecam_va is valid.
                        let orig = unsafe { ecam_read32(ecam_va, bus, dev, func, reg) };

                        // SAFETY: ecam_va is valid; writing to BAR to probe size.
                        unsafe { ecam_write32(ecam_va, bus, dev, func, reg, 0xFFFF_FFFF) };
                        // SAFETY: ecam_va is valid.
                        let sized = unsafe { ecam_read32(ecam_va, bus, dev, func, reg) };
                        // SAFETY: ecam_va is valid; restoring original BAR value.
                        unsafe { ecam_write32(ecam_va, bus, dev, func, reg, orig) };

                        if sized == 0
                        {
                            bar_idx += 1;
                            continue;
                        }

                        let is_mmio = orig & 1 == 0;
                        let is_64bit = is_mmio && (orig >> 1) & 3 == 2;

                        let bar_base;
                        let bar_sz;

                        if is_mmio
                        {
                            let mask = sized & !0xF;
                            bar_sz = (!u64::from(mask)).wrapping_add(1) & 0xFFFF_FFFF;
                            bar_base = u64::from(orig & !0xF);

                            if is_64bit && bar_idx + 1 < 6
                            {
                                let hi_reg = reg + 4;
                                // SAFETY: ecam_va is valid.
                                let hi_orig =
                                    unsafe { ecam_read32(ecam_va, bus, dev, func, hi_reg) };
                                let full_base = bar_base | (u64::from(hi_orig) << 32);

                                if pci_dev.bar_count < MAX_BARS
                                {
                                    pci_dev.bar_phys[pci_dev.bar_count] = full_base;
                                    pci_dev.bar_size[pci_dev.bar_count] = bar_sz;
                                    pci_dev.bar_is_mmio[pci_dev.bar_count] = true;
                                    pci_dev.bar_pci_idx[pci_dev.bar_count] = bar_idx as u8;
                                    pci_dev.bar_count += 1;
                                }
                                bar_idx += 2; // Skip next BAR (upper 32 bits).
                                continue;
                            }
                        }
                        else
                        {
                            let mask = sized & !0x3;
                            bar_sz = u64::from((!mask).wrapping_add(1) & 0xFFFF);
                            bar_base = u64::from(orig & !0x3);
                        }

                        if pci_dev.bar_count < MAX_BARS
                        {
                            pci_dev.bar_phys[pci_dev.bar_count] = bar_base;
                            pci_dev.bar_size[pci_dev.bar_count] = bar_sz;
                            pci_dev.bar_is_mmio[pci_dev.bar_count] = is_mmio;
                            pci_dev.bar_pci_idx[pci_dev.bar_count] = bar_idx as u8;
                            pci_dev.bar_count += 1;
                        }
                        bar_idx += 1;
                    }
                }

                // For VirtIO devices, walk the PCI capability list to locate
                // the modern register regions (common cfg, notify, ISR, device cfg).
                if pci_dev.vendor_id == 0x1AF4
                {
                    // SAFETY: ecam_va is valid (caller guarantee).
                    unsafe { read_virtio_pci_caps(ecam_va, &mut pci_dev) };
                }

                devices[count] = pci_dev;
                count += 1;
            }
        }
        bus = bus.wrapping_add(1);
        if bus == 0
        {
            break; // Wrapped around.
        }
    }

    count
}

/// Check if a PCI device is a `VirtIO` block device.
fn is_virtio_blk(dev: &PciDevice) -> bool
{
    dev.vendor_id == 0x1AF4 && (dev.device_id == 0x1001 || dev.device_id == 0x1042)
}

/// Walk the PCI capability list for a `VirtIO` device and populate
/// `VirtioPciStartupInfo` with the locations of the four register regions.
///
/// # Safety
/// `ecam_va` must be a valid ECAM MMIO mapping covering the device's config space.
unsafe fn read_virtio_pci_caps(ecam_va: u64, dev: &mut PciDevice)
{
    // PCI status register bit 4: capabilities list present.
    // SAFETY: ecam_va is valid (caller guarantee).
    let status = unsafe { ecam_read16(ecam_va, dev.bus, dev.dev, dev.func, 0x06) };
    if status & 0x10 == 0
    {
        return;
    }

    // Capability pointer at offset 0x34 (low 2 bits reserved, must mask).
    // SAFETY: ecam_va is valid.
    let mut cap_ptr = unsafe { ecam_read8(ecam_va, dev.bus, dev.dev, dev.func, 0x34) } & !3;

    let mut iterations = 0u32;
    while cap_ptr != 0 && iterations < 48
    {
        iterations += 1;
        let ptr = u16::from(cap_ptr);

        // SAFETY: ecam_va is valid; cap_ptr is within config space (0-255).
        let cap_id = unsafe { ecam_read8(ecam_va, dev.bus, dev.dev, dev.func, ptr) };
        // SAFETY: ecam_va is valid.
        let next = unsafe { ecam_read8(ecam_va, dev.bus, dev.dev, dev.func, ptr + 1) } & !3;

        // VirtIO vendor-specific capability: cap_id = 0x09.
        if cap_id == 0x09
        {
            // VirtIO PCI cap layout (VirtIO 1.2 §4.1.4):
            //   +0: cap_vndr (0x09)
            //   +1: cap_next
            //   +2: cap_len
            //   +3: cfg_type
            //   +4: bar
            //   +5: id (padding)
            //   +6: padding
            //   +7: padding
            //   +8: offset (u32)
            //   +12: length (u32)
            // SAFETY: ecam_va is valid; offsets within cap structure.
            let cfg_type = unsafe { ecam_read8(ecam_va, dev.bus, dev.dev, dev.func, ptr + 3) };
            // SAFETY: ecam_va is valid; offsets within VirtIO PCI cap structure.
            let bar = unsafe { ecam_read8(ecam_va, dev.bus, dev.dev, dev.func, ptr + 4) };
            // SAFETY: ecam_va is valid; offset within VirtIO PCI cap structure.
            let offset = unsafe { ecam_read32(ecam_va, dev.bus, dev.dev, dev.func, ptr + 8) };
            // SAFETY: ecam_va is valid; offset within VirtIO PCI cap structure.
            let length = unsafe { ecam_read32(ecam_va, dev.bus, dev.dev, dev.func, ptr + 12) };

            let loc = VirtioCapLocation {
                bar,
                pad: [0; 3],
                offset,
                length,
            };

            match cfg_type
            {
                VIRTIO_PCI_CAP_COMMON_CFG => dev.virtio_info.common_cfg = loc,
                VIRTIO_PCI_CAP_NOTIFY_CFG =>
                {
                    dev.virtio_info.notify_cfg = loc;
                    // Notify cap has an extra u32 at offset +16: notify_off_multiplier.
                    // SAFETY: ecam_va is valid; offset within cap structure.
                    dev.virtio_info.notify_off_multiplier =
                        unsafe { ecam_read32(ecam_va, dev.bus, dev.dev, dev.func, ptr + 16) };
                }
                VIRTIO_PCI_CAP_ISR_CFG => dev.virtio_info.isr_cfg = loc,
                VIRTIO_PCI_CAP_DEVICE_CFG => dev.virtio_info.device_cfg = loc,
                _ =>
                {} // PCI cfg access cap (type 5) etc. — not needed.
            }
        }

        cap_ptr = next;
    }
}

// ── MMIO cap splitting ──────────────────────────────────────────────────────

/// Carve an MMIO sub-region cap from a window cap for a specific BAR.
///
/// Uses `mmio_split` to split at BAR boundaries. The window cap is consumed
/// and replaced by the remaining portions. Returns the per-BAR cap slot.
fn split_bar_cap(
    window_slot: &mut u32,
    window_base: &mut u64,
    window_size: &mut u64,
    bar_phys: u64,
    bar_size: u64,
) -> Option<u32>
{
    if bar_phys < *window_base || bar_phys + bar_size > *window_base + *window_size
    {
        return None; // BAR outside window.
    }

    let offset_in_window = bar_phys - *window_base;

    // Split at BAR start if needed.
    let working_slot;
    let working_base;
    let working_size;

    if offset_in_window > 0
    {
        let (lower, upper) = syscall::mmio_split(*window_slot, offset_in_window).ok()?;
        // Lower part is before the BAR — stash it (we lose track of it,
        // which is acceptable for now; future: maintain a free list).
        let _ = lower;
        working_slot = upper;
        working_base = *window_base + offset_in_window;
        working_size = *window_size - offset_in_window;
    }
    else
    {
        working_slot = *window_slot;
        working_base = *window_base;
        working_size = *window_size;
    }

    // Split at BAR end if there's remaining space.
    if bar_size < working_size
    {
        let page_aligned_size = (bar_size + 0xFFF) & !0xFFF;
        if page_aligned_size < working_size
        {
            let (bar_cap, remainder) = syscall::mmio_split(working_slot, page_aligned_size).ok()?;
            *window_slot = remainder;
            *window_base = working_base + page_aligned_size;
            *window_size = working_size - page_aligned_size;
            return Some(bar_cap);
        }
    }

    // BAR consumes the rest of the window.
    *window_size = 0;
    Some(working_slot)
}

// ── Driver spawning ─────────────────────────────────────────────────────────

/// Spawn a driver process with per-device capabilities.
///
/// Requests procmgr to create the process, injects MMIO and IRQ caps, patches
/// `ProcessInfo`, and starts the process.
// too_many_lines: driver spawn is inherently sequential with cap injection,
// ProcessInfo patching, and IPC calls; splitting would fragment the flow.
// too_many_arguments: driver spawning requires per-device BAR caps, IRQ,
// procmgr endpoint, and startup message; splitting would add complexity
// without improving clarity.
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
fn spawn_driver(
    procmgr_ep: u32,
    module_cap: u32,
    self_aspace: u32,
    bar_caps: &[u32],
    bar_bases: &[u64],
    bar_sizes: &[u64],
    irq_cap: Option<u32>,
    irq_id: u32,
    log_ep: u32,
    service_ep: u32,
    virtio_info: &VirtioPciStartupInfo,
    ipc_buf: *mut u64,
)
{
    // Phase 1: CREATE_PROCESS.
    let Ok((reply_label, _)) =
        syscall::ipc_call(procmgr_ep, LABEL_CREATE_PROCESS, 0, &[module_cap])
    else
    {
        log("devmgr: driver CREATE_PROCESS ipc_call failed");
        return;
    };
    if reply_label != 0
    {
        log("devmgr: driver CREATE_PROCESS failed");
        return;
    }

    // SAFETY: IPC buffer is valid.
    let pid = unsafe { core::ptr::read_volatile(ipc_buf) };
    // SAFETY: ipc_buf is the registered IPC buffer.
    #[allow(clippy::cast_ptr_alignment)]
    let (cap_count, reply_caps) = unsafe { syscall::read_recv_caps(ipc_buf) };
    if cap_count < 2
    {
        log("devmgr: driver CREATE_PROCESS reply missing caps");
        return;
    }
    let child_cspace = reply_caps[0];
    let pi_frame = reply_caps[1];

    // Phase 2: Inject per-device caps.
    let mut descs = [CapDescriptor {
        slot: 0,
        cap_type: CapType::Frame,
        pad: [0; 3],
        aux0: 0,
        aux1: 0,
    }; MAX_DRIVER_DESCS];
    let mut desc_count: usize = 0;
    let mut first_slot: u32 = 0;

    // Inject BAR MMIO caps.
    for (i, &bar_cap) in bar_caps.iter().enumerate()
    {
        if let Ok(child_slot) = syscall::cap_copy(bar_cap, child_cspace, !0u64)
        {
            if desc_count == 0
            {
                first_slot = child_slot;
            }
            if desc_count < MAX_DRIVER_DESCS
            {
                descs[desc_count] = CapDescriptor {
                    slot: child_slot,
                    cap_type: CapType::MmioRegion,
                    pad: [0; 3],
                    aux0: bar_bases[i],
                    aux1: bar_sizes[i],
                };
                desc_count += 1;
            }
        }
    }

    // Inject IRQ cap.
    if let Some(irq_slot) = irq_cap
    {
        if let Ok(irq_intermediary) = syscall::cap_derive(irq_slot, !0u64)
        {
            if let Ok(child_slot) = syscall::cap_copy(irq_intermediary, child_cspace, !0u64)
            {
                if desc_count == 0
                {
                    first_slot = child_slot;
                }
                if desc_count < MAX_DRIVER_DESCS
                {
                    descs[desc_count] = CapDescriptor {
                        slot: child_slot,
                        cap_type: CapType::Interrupt,
                        pad: [0; 3],
                        aux0: u64::from(irq_id),
                        aux1: 0,
                    };
                    desc_count += 1;
                }
            }
        }
    }

    // Inject procmgr endpoint so driver can request frames.
    if let Ok(ep_derived) = syscall::cap_derive(procmgr_ep, !0u64)
    {
        if let Ok(child_slot) = syscall::cap_copy(ep_derived, child_cspace, !0u64)
        {
            if desc_count == 0
            {
                first_slot = child_slot;
            }
            if desc_count < MAX_DRIVER_DESCS
            {
                descs[desc_count] = CapDescriptor {
                    slot: child_slot,
                    cap_type: CapType::Frame, // sentinel for endpoint
                    pad: [0; 3],
                    aux0: 0,
                    aux1: 0,
                };
                desc_count += 1;
            }
        }
    }

    // Inject log endpoint cap (sentinel: Frame with aux0=LOG_ENDPOINT_SENTINEL).
    if log_ep != 0
    {
        if let Ok(log_derived) = syscall::cap_derive(log_ep, !0u64)
        {
            if let Ok(child_slot) = syscall::cap_copy(log_derived, child_cspace, !0u64)
            {
                if desc_count == 0
                {
                    first_slot = child_slot;
                }
                if desc_count < MAX_DRIVER_DESCS
                {
                    descs[desc_count] = CapDescriptor {
                        slot: child_slot,
                        cap_type: CapType::Frame,
                        pad: [0; 3],
                        aux0: LOG_ENDPOINT_SENTINEL,
                        aux1: 0,
                    };
                    desc_count += 1;
                }
            }
        }
    }

    // Inject service endpoint cap (sentinel: Frame with aux0=SERVICE_ENDPOINT_SENTINEL).
    if service_ep != 0
    {
        if let Ok(child_slot) = syscall::cap_copy(service_ep, child_cspace, !0u64)
        {
            if desc_count == 0
            {
                first_slot = child_slot;
            }
            if desc_count < MAX_DRIVER_DESCS
            {
                descs[desc_count] = CapDescriptor {
                    slot: child_slot,
                    cap_type: CapType::Frame,
                    pad: [0; 3],
                    aux0: SERVICE_ENDPOINT_SENTINEL,
                    aux1: 0,
                };
                desc_count += 1;
            }
        }
    }

    // Phase 3: Patch ProcessInfo.
    if syscall::mem_map(
        pi_frame,
        self_aspace,
        DRIVER_PI_VA,
        0,
        1,
        syscall::PROT_WRITE,
    )
    .is_err()
    {
        log("devmgr: cannot map driver ProcessInfo");
        return;
    }

    // SAFETY: DRIVER_PI_VA is mapped writable to the ProcessInfo page.
    #[allow(clippy::cast_ptr_alignment)]
    let pi = unsafe { &mut *(DRIVER_PI_VA as *mut ProcessInfo) };

    pi.initial_caps_base = first_slot;
    pi.initial_caps_count = desc_count as u32;
    pi.cap_descriptor_count = desc_count as u32;

    let descs_offset = core::mem::size_of::<ProcessInfo>() as u32;
    let descs_offset_aligned = (descs_offset + 7) & !7;
    pi.cap_descriptors_offset = descs_offset_aligned;

    let desc_size = core::mem::size_of::<CapDescriptor>();
    for (i, desc) in descs.iter().enumerate().take(desc_count)
    {
        let byte_offset = descs_offset_aligned as usize + i * desc_size;
        if byte_offset + desc_size > PAGE_SIZE as usize
        {
            break;
        }
        // SAFETY: byte_offset is within the mapped page; descs_offset_aligned
        // is 8-byte aligned and CapDescriptor is 24 bytes.
        #[allow(clippy::cast_ptr_alignment)]
        unsafe {
            let ptr = (DRIVER_PI_VA as *mut u8)
                .add(byte_offset)
                .cast::<CapDescriptor>();
            core::ptr::write(ptr, *desc);
        }
    }

    // Write VirtIO PCI startup message after cap descriptors.
    let msg_offset = descs_offset_aligned as usize + desc_count * desc_size;
    let msg_offset_aligned = (msg_offset + 7) & !7;
    if msg_offset_aligned + VirtioPciStartupInfo::SIZE <= PAGE_SIZE as usize
    {
        // SAFETY: byte range is within the mapped page.
        let msg_buf = unsafe {
            core::slice::from_raw_parts_mut(
                (DRIVER_PI_VA as *mut u8).add(msg_offset_aligned),
                VirtioPciStartupInfo::SIZE,
            )
        };
        let _ = virtio_info.to_bytes(msg_buf);
        pi.startup_message_offset = msg_offset_aligned as u32;
        pi.startup_message_len = VirtioPciStartupInfo::SIZE as u32;
    }

    let _ = syscall::mem_unmap(self_aspace, DRIVER_PI_VA, 1);

    // Phase 4: START_PROCESS.
    // SAFETY: writing pid to IPC buffer.
    unsafe { core::ptr::write_volatile(ipc_buf, pid) };
    match syscall::ipc_call(procmgr_ep, LABEL_START_PROCESS, 1, &[])
    {
        Ok((0, _)) => log("devmgr: driver started"),
        _ => log("devmgr: driver START_PROCESS failed"),
    }
}

// ── Entry point ─────────────────────────────────────────────────────────────

#[no_mangle]
#[allow(clippy::too_many_lines)]
extern "Rust" fn main(startup: &StartupInfo) -> !
{
    // Register IPC buffer.
    if syscall::ipc_buffer_set(startup.ipc_buffer as u64).is_err()
    {
        syscall::thread_exit();
    }

    // Classify received capabilities.
    let mut caps = DevmgrCaps::new();
    classify_caps(startup, &mut caps);

    // Initialise IPC logging (must be after IPC buffer and cap classification).
    if caps.log_ep != 0
    {
        // SAFETY: single-threaded; called once before any log calls.
        unsafe { runtime::log::log_init(caps.log_ep, startup.ipc_buffer) };
    }

    // SAFETY: IPC buffer is page-aligned.
    #[allow(clippy::cast_ptr_alignment)]
    let ipc_buf = startup.ipc_buffer.cast::<u64>();

    // PCI device discovery (architecture-independent; UEFI provides ECAM on both
    // x86-64 and RISC-V).
    if caps.ecam_slot == 0
    {
        log("devmgr: no PCI ECAM capability, halting");
        halt_loop();
    }

    // Map ECAM region.
    let ecam_pages = caps.ecam_size.div_ceil(PAGE_SIZE);
    if syscall::mmio_map(caps.self_aspace, caps.ecam_slot, MMIO_MAP_VA, 0).is_err()
    {
        log("devmgr: failed to map ECAM region");
        halt_loop();
    }
    log("devmgr: ECAM mapped ok");
    log_hex("devmgr: ECAM base=", caps.ecam_base);
    log_hex("devmgr: ECAM size=", caps.ecam_size);

    let start_bus = 0u8;
    let end_bus = ((caps.ecam_size / (256 * 4096)).min(256) - 1) as u8;

    let mut devices = [PciDevice {
        bus: 0,
        dev: 0,
        func: 0,
        vendor_id: 0,
        device_id: 0,
        bar_phys: [0; MAX_BARS],
        bar_size: [0; MAX_BARS],
        bar_is_mmio: [false; MAX_BARS],
        bar_pci_idx: [0; MAX_BARS],
        bar_count: 0,
        irq_line: 0,
        virtio_info: VirtioPciStartupInfo::default(),
    }; MAX_DEVICES];

    // SAFETY: MMIO_MAP_VA is a valid ECAM mapping.
    let dev_count = unsafe { pci_enumerate(MMIO_MAP_VA, start_bus, end_bus, &mut devices) };

    log_hex("devmgr: PCI devices found: ", dev_count as u64);

    // Unmap ECAM.
    let _ = syscall::mem_unmap(caps.self_aspace, MMIO_MAP_VA, ecam_pages);

    // Create block device service endpoint for the driver to receive on.
    let blk_ep = syscall::cap_create_endpoint().unwrap_or(0);
    if blk_ep == 0
    {
        log("devmgr: failed to create block device endpoint");
    }

    // Track whether we successfully spawned a block device driver.
    let mut blk_driver_spawned = false;

    // Find and spawn VirtIO block device driver.
    for pci_dev in devices.iter().take(dev_count)
    {
        if !is_virtio_blk(pci_dev)
        {
            continue;
        }

        log("devmgr: found virtio-blk PCI device");
        log_hex("devmgr: IRQ line=", u64::from(pci_dev.irq_line));

        if caps.driver_module_count == 0
        {
            log("devmgr: no driver modules available");
            break;
        }

        // Find the BAR that VirtIO capabilities reference (common_cfg.bar).
        let virtio_bar_idx = pci_dev.virtio_info.common_cfg.bar;
        let mut bar_caps = [0u32; 1];
        let mut bar_bases = [0u64; 1];
        let mut bar_sizes = [0u64; 1];
        let mut bar_cap_count = 0;

        // Match the VirtIO BAR index against discovered BARs using bar_pci_idx.
        for b in 0..pci_dev.bar_count
        {
            if pci_dev.bar_pci_idx[b] != virtio_bar_idx || !pci_dev.bar_is_mmio[b]
            {
                continue;
            }
            log_hex("devmgr: VirtIO BAR phys=", pci_dev.bar_phys[b]);
            log_hex("devmgr: VirtIO BAR size=", pci_dev.bar_size[b]);

            // Try each PCI MMIO window to find one containing this BAR.
            for w in 0..caps.pci_mmio_window_count
            {
                let win = &mut caps.pci_mmio_windows[w];
                if win.size == 0
                {
                    continue;
                }
                if let Some(cap) = split_bar_cap(
                    &mut win.slot,
                    &mut win.base,
                    &mut win.size,
                    pci_dev.bar_phys[b],
                    pci_dev.bar_size[b],
                )
                {
                    bar_caps[0] = cap;
                    bar_bases[0] = pci_dev.bar_phys[b];
                    bar_sizes[0] = pci_dev.bar_size[b];
                    bar_cap_count = 1;
                    break;
                }
            }
            break;
        }
        if bar_cap_count == 0
        {
            log("devmgr: VirtIO BAR not found in PCI windows");
            log_hex("devmgr:   virtio_bar_idx=", u64::from(virtio_bar_idx));
        }

        // Find matching IRQ cap.
        let mut irq_cap = None;
        let mut irq_id = 0u32;
        for j in 0..caps.irq_count
        {
            if caps.irq_slots[j].1 == u32::from(pci_dev.irq_line)
            {
                irq_cap = Some(caps.irq_slots[j].0);
                irq_id = caps.irq_slots[j].1;
                break;
            }
        }

        // Module 0 in driver_module_slots = virtio-blk (module index 3).
        let module_cap = caps.driver_module_slots[0];

        spawn_driver(
            caps.procmgr_ep,
            module_cap,
            caps.self_aspace,
            &bar_caps[..bar_cap_count],
            &bar_bases[..bar_cap_count],
            &bar_sizes[..bar_cap_count],
            irq_cap,
            irq_id,
            caps.log_ep,
            blk_ep,
            &pci_dev.virtio_info,
            ipc_buf,
        );
        blk_driver_spawned = true;

        break; // Only spawn for the first virtio-blk device.
    }

    // VirtIO MMIO transport probing is reserved for non-UEFI platforms
    // where DTB provides virtio,mmio device nodes. On UEFI (both x86-64
    // and RISC-V), VirtIO devices are PCI-based and discovered above.

    // ── Device registry IPC ───────────────────────────────────────────

    if caps.registry_ep == 0
    {
        log("devmgr: no registry endpoint injected, halting");
        halt_loop();
    }

    log("devmgr: enumeration complete, entering registry loop");
    loop
    {
        let Ok((label, _)) = syscall::ipc_recv(caps.registry_ep)
        else
        {
            continue;
        };

        match label
        {
            LABEL_QUERY_BLOCK_DEVICE =>
            {
                if blk_driver_spawned && blk_ep != 0
                {
                    // Derive a copy of the block device endpoint for the client.
                    if let Ok(derived) = syscall::cap_derive(blk_ep, !0u64)
                    {
                        let _ = syscall::ipc_reply(0, 0, &[derived]);
                    }
                    else
                    {
                        let _ = syscall::ipc_reply(1, 0, &[]);
                    }
                }
                else
                {
                    // No block device found.
                    let _ = syscall::ipc_reply(1, 0, &[]);
                }
            }
            _ =>
            {
                // Unknown label.
                let _ = syscall::ipc_reply(0xFF, 0, &[]);
            }
        }
    }
}

fn halt_loop() -> !
{
    loop
    {
        let _ = syscall::thread_yield();
    }
}
