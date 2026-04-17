// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// devmgr/src/caps.rs

//! Bootstrap cap acquisition for devmgr.
//!
//! Acquires the hardware capability set from init over IPC, assembling it
//! into typed fields for use by PCI enumeration and driver spawning.

use ipc::IpcBuf;
use process_abi::StartupInfo;

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
    /// devmgr's own bootstrap endpoint (receives bootstrap requests from
    /// drivers it spawns).
    pub self_bootstrap_ep: u32,
    pub self_aspace: u32,
    pub driver_module_slots: [u32; 8],
    pub driver_module_count: usize,
}

impl DevmgrCaps
{
    pub fn new(startup: &StartupInfo) -> Self
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
            self_bootstrap_ep: 0,
            self_aspace: startup.self_aspace,
            driver_module_slots: [0; 8],
            driver_module_count: 0,
        }
    }
}

// ── Bootstrap plan layout (init → devmgr) ──────────────────────────────────
//
// Round 1 (4 caps, 0 data): [log_ep, registry_ep, procmgr_ep, ecam_cap]
//   data words:
//     word 0: ecam_base
//     word 1: ecam_size
//
// Round 2 (up to 2 caps, up to 4 data words): PCI MMIO windows
//   data[0] = window count (N); then N * (slot, base, size) groups follow
//     caps[0..N]: MMIO region slots
//     word 1: base0; word 2: size0; word 3: base1; word 4: size1
//
// Round 3..: IRQ caps, up to 4 per round; data words carry irq_id per cap
//   word 0: count of IRQs in this round
//   word 1..N+1: irq_id for caps[0..N-1]
//   final round if done=true.
//
// Round N: driver module caps, up to 4 per round, no data words.
//   First module cap is always the virtio-blk module (module index 3 today);
//   additional modules may follow in later releases.

/// Pull devmgr's initial cap set from init via multi-round bootstrap.
#[allow(clippy::too_many_lines)]
pub fn bootstrap_caps(startup: &StartupInfo, ipc: IpcBuf) -> Option<DevmgrCaps>
{
    let mut caps = DevmgrCaps::new(startup);
    let creator = startup.creator_endpoint;
    if creator == 0
    {
        return None;
    }

    // Round 1: endpoints + ECAM.
    let round1 = ipc::bootstrap::request_round(creator, ipc).ok()?;
    if round1.cap_count < 4
    {
        return None;
    }
    caps.log_ep = round1.caps[0];
    caps.registry_ep = round1.caps[1];
    caps.procmgr_ep = round1.caps[2];
    caps.ecam_slot = round1.caps[3];
    caps.ecam_base = ipc.read_word(0);
    caps.ecam_size = ipc.read_word(1);

    // Round 2: PCI MMIO windows.
    let round2 = ipc::bootstrap::request_round(creator, ipc).ok()?;
    let win_count = ipc.read_word(0) as usize;
    for i in 0..win_count.min(round2.cap_count).min(2)
    {
        caps.pci_mmio_windows[i] = PciMmioWindow {
            slot: round2.caps[i],
            base: ipc.read_word(1 + i * 2),
            size: ipc.read_word(2 + i * 2),
        };
        caps.pci_mmio_window_count += 1;
    }

    // IRQ rounds: loop until one of them is marked done=true.
    let mut irq_done = round2.done;
    while !irq_done
    {
        let round = ipc::bootstrap::request_round(creator, ipc).ok()?;
        // First word in this round is round-kind tag: 0 = IRQ round, 1 = module round.
        let kind = ipc.read_word(0);
        irq_done = round.done;
        if kind == 0
        {
            // IRQ caps: data[1..cap_count+1] = irq_id for each cap.
            for i in 0..round.cap_count.min(4)
            {
                if caps.irq_count < caps.irq_slots.len()
                {
                    let irq_id = ipc.read_word(1 + i) as u32;
                    caps.irq_slots[caps.irq_count] = (round.caps[i], irq_id);
                    caps.irq_count += 1;
                }
            }
        }
        else
        {
            // Module caps, no per-cap data.
            for i in 0..round.cap_count.min(4)
            {
                if caps.driver_module_count < caps.driver_module_slots.len()
                {
                    caps.driver_module_slots[caps.driver_module_count] = round.caps[i];
                    caps.driver_module_count += 1;
                }
            }
        }
        if irq_done
        {
            break;
        }
    }

    // devmgr creates its own bootstrap endpoint for serving drivers.
    caps.self_bootstrap_ep = syscall::cap_create_endpoint().ok()?;

    Some(caps)
}
