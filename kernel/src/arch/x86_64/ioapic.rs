// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/arch/x86_64/ioapic.rs

//! Minimal I/O APIC driver for x86-64.
//!
//! Programs interrupt redirection entries so device IRQs (GSIs) are delivered
//! to the CPU as IDT vectors. Only a single I/O APIC is supported.
//!
//! # Hardware interface
//! The I/O APIC is memory-mapped at [`IOAPIC_BASE_PHYS`], accessible via
//! the kernel direct map. Two 32-bit registers control access:
//! - `IOREGSEL` (offset 0x00): index of the register to read/write.
//! - `IOWIN`    (offset 0x10): data window for the selected register.
//!
//! Redirection entries are 64-bit values spanning two 32-bit registers:
//! - Low  dword at index `0x10 + 2 * gsi`
//! - High dword at index `0x11 + 2 * gsi`
//!
//! # Vector assignment
//! GSI `n` is assigned to IDT vector `DEVICE_VECTOR_BASE + n` (33 + n).
//! This keeps the mapping trivial and avoids a vector allocator.
//! Vectors 33–55 cover 23 IOAPIC inputs (typical Q35 has 24 entries).
//!
//! # Limitations / deferred work
//!
//! - **Single IOAPIC only.** Real machines may have multiple I/O APICs.
//!   // TODO: Discover all IOAPICs from the ACPI MADT and maintain a per-IOAPIC
//!   // table. Pick up alongside ACPI parsing.
//!
//! - **Hardcoded base address.** QEMU Q35 always puts the I/O APIC at
//!   `0xFEC0_0000`. On real hardware this may differ.
//!   // TODO: Read IOAPIC base from ACPI MADT IOAPIC record. Pick up when
//!   // ACPI table parsing is added.
//!
//! - **No MSI/MSI-X support.** Required for modern `PCIe` devices.
//!   // TODO: Add MSI/MSI-X programming when `PCIe` enumeration is implemented.
//!
//! - **Edge-triggered, active-high only.** Level-triggered and active-low
//!   sources (some legacy ISA IRQs via PCI interrupt routing) are not handled.
//!   Add `flags` parsing from the `InterruptObject` when needed.
//!
//! # Modification notes
//! - To add a new GSI: `route(gsi, DEVICE_VECTOR_BASE + gsi as u8)` then
//!   `unmask(gsi)` after registering a signal handler.
//! - To support level-triggered IRQs: set bit 15 (level-sensitive) and
//!   bit 13 (active-low polarity) in the redirection entry low dword.

// cast_possible_truncation: u64→usize APIC MMIO address arithmetic; bounded by APIC layout.
// cast_lossless: u8→u32 vector widening casts.
#![allow(clippy::cast_possible_truncation, clippy::cast_lossless)]

use crate::mm::paging::DIRECT_MAP_BASE;

// ── Hardware constants ────────────────────────────────────────────────────────

/// Physical base address of the I/O APIC (standard Q35 / QEMU location).
const IOAPIC_BASE_PHYS: u64 = 0xFEC0_0000;

/// Register select offset (write GSI index here).
const IOREGSEL: usize = 0x00;
/// Data window offset (read/write data here after selecting register).
const IOWIN: usize = 0x10;

/// I/O APIC identification register.
const IOAPICID: u32 = 0x00;
/// I/O APIC version register (bits [23:16] = max redirection entry index).
const IOAPICVER: u32 = 0x01;

/// Base IDT vector for device IRQs.
/// GSI `n` maps to vector `DEVICE_VECTOR_BASE + n`.
pub const DEVICE_VECTOR_BASE: u8 = 33;

/// Mask bit in the low dword of a redirection entry (bit 16).
const REDIR_MASK: u32 = 1 << 16;

/// Fixed delivery mode (000), physical destination, vector in [7:0].
/// Logical destination mode would be bit 11; we leave it clear (physical).
const REDIR_FIXED: u32 = 0x0000_0000;

// ── Register access ───────────────────────────────────────────────────────────

/// Write `val` to IOAPIC register `reg`.
///
/// # Safety
/// Must only be called after Phase 3 (direct map active).
unsafe fn ioapic_write(reg: u32, val: u32)
{
    let base = (DIRECT_MAP_BASE + IOAPIC_BASE_PHYS) as usize;
    // SAFETY: IOAPIC_BASE_PHYS is a valid kernel mapping via direct map at
    // DIRECT_MAP_BASE; IOREGSEL/IOWIN offsets are within IOAPIC register range;
    // volatile ensures proper ordering of register select and data writes.
    unsafe {
        core::ptr::write_volatile((base + IOREGSEL) as *mut u32, reg);
        core::ptr::write_volatile((base + IOWIN) as *mut u32, val);
    }
}

/// Read IOAPIC register `reg`.
///
/// # Safety
/// Must only be called after Phase 3 (direct map active).
unsafe fn ioapic_read(reg: u32) -> u32
{
    let base = (DIRECT_MAP_BASE + IOAPIC_BASE_PHYS) as usize;
    // SAFETY: IOAPIC_BASE_PHYS is a valid kernel mapping via direct map at
    // DIRECT_MAP_BASE; IOREGSEL/IOWIN offsets are within IOAPIC register range;
    // volatile ensures proper ordering of register select and data read.
    unsafe {
        core::ptr::write_volatile((base + IOREGSEL) as *mut u32, reg);
        core::ptr::read_volatile((base + IOWIN) as *const u32)
    }
}

// ── Public interface ──────────────────────────────────────────────────────────

/// Initialise the I/O APIC: mask all redirection entries.
///
/// Called once during Phase 5 boot after the direct map is active.
///
/// # Safety
/// Must be called from a single-threaded context after Phase 3 completes.
#[cfg(not(test))]
pub unsafe fn init()
{
    // Read the version register to determine the number of redirection entries.
    // Bits [23:16] hold (max_entry_index), so num_entries = max_index + 1.
    // SAFETY: single-threaded init phase after Phase 3; direct map is active.
    let ver = unsafe { ioapic_read(IOAPICVER) };
    let max_entry = (ver >> 16) & 0xFF;

    // SAFETY: single-threaded init phase; reading IOAPICID register.
    let ioapic_id = unsafe { ioapic_read(IOAPICID) };
    crate::kprintln!(
        "ioapic: base={:#x} id={:#x} max_redir={}",
        IOAPIC_BASE_PHYS,
        ioapic_id,
        max_entry
    );

    // Mask all entries (bit 16 = interrupt mask = 1).
    for gsi in 0..=max_entry
    {
        // SAFETY: single-threaded init phase; programming redirection entries
        // to masked state; no concurrent access or IRQ delivery possible.
        unsafe {
            ioapic_write(0x10 + 2 * gsi, REDIR_MASK);
            ioapic_write(0x11 + 2 * gsi, 0);
        }
    }
}

/// Program a redirection entry for `gsi` to deliver vector `vector`.
///
/// The entry is programmed masked; call [`unmask`] when ready to receive.
/// Uses edge-triggered, active-high, fixed delivery to LAPIC 0.
///
/// # Safety
/// Must only be called after [`init`].
#[cfg(not(test))]
pub unsafe fn route(gsi: u32, vector: u8)
{
    // Low dword: vector | fixed delivery | masked.
    // High dword: destination LAPIC ID 0 in bits [27:24].
    let low = REDIR_MASK | REDIR_FIXED | (vector as u32);
    let high: u32 = 0; // dest LAPIC ID 0

    // SAFETY: caller ensures init() has completed; programming redirection entry
    // for specified GSI with masked delivery; entry remains masked until unmask().
    unsafe {
        ioapic_write(0x10 + 2 * gsi, low);
        ioapic_write(0x11 + 2 * gsi, high);
    }
}

/// Mask (suppress delivery of) the redirection entry for `gsi`.
///
/// # Safety
/// Must only be called after [`init`].
#[cfg(not(test))]
pub unsafe fn mask(gsi: u32)
{
    let reg = 0x10 + 2 * gsi;
    // SAFETY: caller ensures init() has completed; reading current redirection entry.
    let current = unsafe { ioapic_read(reg) };
    // SAFETY: setting mask bit in redirection entry; serializes with IRQ dispatch.
    unsafe {
        ioapic_write(reg, current | REDIR_MASK);
    }
}

/// Unmask (enable delivery of) the redirection entry for `gsi`.
///
/// # Safety
/// Must only be called after [`init`] and after [`route`] has programmed the entry.
#[cfg(not(test))]
pub unsafe fn unmask(gsi: u32)
{
    let reg = 0x10 + 2 * gsi;
    // SAFETY: caller ensures init() and route() have completed; reading current entry.
    let current = unsafe { ioapic_read(reg) };
    // SAFETY: clearing mask bit enables IRQ delivery; caller must have registered handler.
    unsafe {
        ioapic_write(reg, current & !REDIR_MASK);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn ioapic_base_phys_constant()
    {
        assert_eq!(IOAPIC_BASE_PHYS, 0xFEC0_0000);
    }

    #[test]
    fn device_vector_base_is_33()
    {
        assert_eq!(DEVICE_VECTOR_BASE, 33);
    }

    #[test]
    fn redir_mask_bit_is_16()
    {
        assert_eq!(REDIR_MASK, 1 << 16);
    }

    #[test]
    fn redirection_entry_low_encoding()
    {
        // For GSI 0 with vector 33:
        // low = REDIR_MASK | 33 = 0x0001_0021
        let vector: u8 = 33;
        let low = REDIR_MASK | REDIR_FIXED | (vector as u32);
        assert_eq!(low & 0xFF, 33, "vector in bits [7:0]");
        assert!(low & REDIR_MASK != 0, "entry starts masked");
    }
}
