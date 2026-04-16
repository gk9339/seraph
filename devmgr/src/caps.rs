// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// devmgr/src/caps.rs

//! Capability classification for devmgr startup.
//!
//! Iterates the initial capability descriptors from `StartupInfo` and sorts
//! them into typed fields for use by PCI enumeration and driver spawning.

use process_abi::{CapType, StartupInfo};

pub struct PciMmioWindow
{
    pub slot: u32,
    pub base: u64,
    pub size: u64,
}

pub struct DevmgrCaps
{
    pub ecam_slot: u32,
    pub ecam_base: u64,
    pub ecam_size: u64,
    /// PCI MMIO windows (32-bit and optionally 64-bit).
    pub pci_mmio_windows: [PciMmioWindow; 2],
    pub pci_mmio_window_count: usize,
    pub irq_slots: [(u32, u32); 64], // (slot, irq_id)
    pub irq_count: usize,
    pub procmgr_ep: u32,
    pub log_ep: u32,
    pub registry_ep: u32,
    pub self_aspace: u32,
    pub self_thread: u32,
    pub driver_module_slots: [u32; 8],
    pub driver_module_count: usize,
}

impl DevmgrCaps
{
    pub fn new() -> Self
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

/// Classify initial capabilities from startup info into typed fields.
pub fn classify_caps(startup: &StartupInfo, caps: &mut DevmgrCaps)
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
                if d.aux0 == ipc::LOG_ENDPOINT_SENTINEL
                {
                    caps.log_ep = d.slot;
                }
                else if d.aux0 == ipc::REGISTRY_ENDPOINT_SENTINEL
                {
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
                    caps.driver_module_slots[caps.driver_module_count] = d.slot;
                    caps.driver_module_count += 1;
                }
            }
            CapType::SbiControl =>
            {}
        }
    }
}
