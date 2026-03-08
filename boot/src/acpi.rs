// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// boot/src/acpi.rs

//! Minimal ACPI table parser: RSDP → XSDT → MADT, MCFG, SPCR.
//!
//! Reads tables in-place from identity-mapped physical memory. No allocation.
//! Architecture-neutral: runs on any platform where `acpi_rsdp != 0`.
//!
//! Error handling: malformed tables log a warning and return partial results.
//! Only ACPI 2.0+ (XSDT-based) is supported; ACPI 1.0 (RSDT-only) is skipped.
//!
//! # Extending
//! To parse additional ACPI tables, add a new arm to the `match sig` block in
//! `parse_acpi_resources`. Tables are identified by their 4-byte ASCII signature.

use crate::bprintln;
use boot_protocol::{PlatformResource, ResourceType};

// ── Layout constants ──────────────────────────────────────────────────────────

// RSDP (ACPI 2.0, offset from base):
//   0: signature[8]  8: checksum  9: oemid[6]  15: revision
//  16: rsdt_address(u32)  20: length(u32)  24: xsdt_address(u64)
//  32: extended_checksum  33: reserved[3]

const RSDP_SIG: &[u8; 8] = b"RSD PTR ";
const RSDP_OFF_REVISION: usize = 15;
const RSDP_OFF_XSDT: usize = 24;

// SDT header (36 bytes, common to all ACPI description tables):
//   0: signature[4]  4: length(u32)  8: revision  9: checksum
//  10: oemid[6]  16: oemtableid[8]  24: oemrev(u32)  28: creatorid(u32)
//  32: creatorrev(u32)

const SDT_HDR_LEN: usize = 36;
const SDT_OFF_SIGNATURE: usize = 0;
const SDT_OFF_LENGTH: usize = 4;

// MADT offsets (relative to table start, after SDT header):
const MADT_OFF_LAPIC_BASE: usize = SDT_HDR_LEN; // u32
                                                // MADT entries start at offset 44.
const MADT_ENTRIES_OFF: usize = 44;

// MADT entry types:
const MADT_TYPE_IOAPIC: u8 = 1;
const MADT_TYPE_ISO: u8 = 2;

// MCFG: entries start at offset 44 (SDT_HDR_LEN + 8 reserved bytes).
const MCFG_ENTRIES_OFF: usize = SDT_HDR_LEN + 8;
const MCFG_ENTRY_SIZE: usize = 16;

// SPCR GAS base_address field offset from table start:
//   36: interface_type(u8)  37-39: reserved  40: GAS(12 bytes)
//   GAS layout: address_space_id(u8), bit_width(u8), bit_offset(u8),
//               access_size(u8), address(u64)
const SPCR_OFF_IFACE: usize = SDT_HDR_LEN;
const SPCR_OFF_GAS: usize = SDT_HDR_LEN + 4;
const SPCR_GAS_ADDR_SPACE_ID: usize = SPCR_OFF_GAS; // u8, 0=MMIO
const SPCR_GAS_ADDRESS: usize = SPCR_OFF_GAS + 4; // u64

// ── Byte-level read helpers ───────────────────────────────────────────────────

/// Read a little-endian u32 at byte `off` within `buf`. Returns 0 on short read.
fn read_u32(buf: &[u8], off: usize) -> u32
{
    if off + 4 > buf.len()
    {
        return 0;
    }
    u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

/// Read a little-endian u64 at byte `off` within `buf`. Returns 0 on short read.
fn read_u64(buf: &[u8], off: usize) -> u64
{
    if off + 8 > buf.len()
    {
        return 0;
    }
    u64::from_le_bytes([
        buf[off],
        buf[off + 1],
        buf[off + 2],
        buf[off + 3],
        buf[off + 4],
        buf[off + 5],
        buf[off + 6],
        buf[off + 7],
    ])
}

/// Read a u8 at byte `off` within `buf`. Returns 0 on short read.
fn read_u8(buf: &[u8], off: usize) -> u8
{
    buf.get(off).copied().unwrap_or(0)
}

/// Read a u16 little-endian at byte `off` within `buf`. Returns 0 on short read.
fn read_u16(buf: &[u8], off: usize) -> u16
{
    if off + 2 > buf.len()
    {
        return 0;
    }
    u16::from_le_bytes([buf[off], buf[off + 1]])
}

/// Return a byte slice view of `len` bytes at physical address `phys`.
///
/// # Safety
/// `phys` must be a valid, identity-mapped physical address with at least
/// `len` accessible bytes. The caller must ensure the region lives long enough.
unsafe fn phys_slice<'a>(phys: u64, len: usize) -> &'a [u8]
{
    unsafe { core::slice::from_raw_parts(phys as *const u8, len) }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Parse ACPI tables starting from `rsdp_addr` and write [`PlatformResource`]
/// entries into `out`. Returns the number of entries written.
///
/// Non-fatal: malformed tables log a warning and yield partial results.
///
/// Resources extracted:
/// - MADT: local APIC MMIO base, I/O APIC MmioRanges, interrupt ISOs (IrqLine)
/// - MCFG: PCI ECAM windows (PciEcam)
/// - RSDP region: PlatformTable
/// - x86-64 legacy PCI I/O ports 0xCF8–0xCFF (IoPortRange, only on x86_64)
///
/// # Safety
/// `rsdp_addr` must be a physical address of a valid, identity-mapped ACPI RSDP.
pub unsafe fn parse_acpi_resources(rsdp_addr: u64, out: &mut [PlatformResource]) -> usize
{
    let mut count = 0;

    /// Push a PlatformResource into `out` if space remains.
    macro_rules! push {
        ($res:expr) => {
            if count < out.len()
            {
                out[count] = $res;
                count += 1;
            }
        };
    }

    // ── Validate RSDP ─────────────────────────────────────────────────────────

    // RSDP v2.0 is 36 bytes; read enough for all needed fields.
    let rsdp = unsafe { phys_slice(rsdp_addr, 36) };
    if &rsdp[..8] != RSDP_SIG
    {
        bprintln!("seraph-boot:     ACPI: bad RSDP signature, skipping");
        return 0;
    }
    if read_u8(rsdp, RSDP_OFF_REVISION) < 2
    {
        bprintln!("seraph-boot:     ACPI: RSDP revision < 2 (no XSDT), skipping");
        return 0;
    }
    let xsdt_addr = read_u64(rsdp, RSDP_OFF_XSDT);
    if xsdt_addr == 0
    {
        bprintln!("seraph-boot:     ACPI: XSDT address is zero, skipping");
        return 0;
    }

    // Record the RSDP blob as a PlatformTable (id=0: RSDP).
    push!(PlatformResource {
        resource_type: ResourceType::PlatformTable,
        flags: 0,
        base: rsdp_addr,
        size: 36,
        id: 0,
    });

    // ── Validate XSDT ─────────────────────────────────────────────────────────

    let xsdt_hdr = unsafe { phys_slice(xsdt_addr, SDT_HDR_LEN) };
    if &xsdt_hdr[SDT_OFF_SIGNATURE..SDT_OFF_SIGNATURE + 4] != b"XSDT"
    {
        bprintln!("seraph-boot:     ACPI: bad XSDT signature, skipping subtables");
        return count;
    }
    let xsdt_len = read_u32(xsdt_hdr, SDT_OFF_LENGTH) as usize;
    if xsdt_len < SDT_HDR_LEN
    {
        bprintln!("seraph-boot:     ACPI: XSDT length too small, skipping subtables");
        return count;
    }
    let xsdt = unsafe { phys_slice(xsdt_addr, xsdt_len) };

    // XSDT entries: array of u64 physical addresses starting after the header.
    let entries_bytes = &xsdt[SDT_HDR_LEN..];
    let entry_count = entries_bytes.len() / 8;

    // Track if MADT was found (used to conditionally add legacy x86 I/O ports).
    let mut found_madt = false;

    for i in 0..entry_count
    {
        let table_addr = read_u64(entries_bytes, i * 8);
        if table_addr == 0
        {
            continue;
        }

        // Read just the SDT header to get signature and length.
        let hdr = unsafe { phys_slice(table_addr, SDT_HDR_LEN) };
        let sig = &hdr[SDT_OFF_SIGNATURE..SDT_OFF_SIGNATURE + 4];
        let table_len = read_u32(hdr, SDT_OFF_LENGTH) as usize;
        if table_len < SDT_HDR_LEN
        {
            continue;
        }
        let table = unsafe { phys_slice(table_addr, table_len) };

        match sig
        {
            b"APIC" =>
            {
                // MADT: local APIC base + I/O APICs + ISOs.
                found_madt = true;
                parse_madt(table, table_addr, &mut count, out);
            }
            b"MCFG" =>
            {
                parse_mcfg(table, &mut count, out);
            }
            b"SPCR" =>
            {
                // SPCR is parsed separately for UART discovery; here just
                // record it as PlatformTable (id=1: SPCR) for devmgr.
                push!(PlatformResource {
                    resource_type: ResourceType::PlatformTable,
                    flags: 0,
                    base: table_addr,
                    size: table_len as u64,
                    id: 1,
                });
            }
            _ =>
            {} // Skip unknown tables gracefully.
        }
    }

    // x86-64 legacy PCI configuration I/O ports 0xCF8–0xCFF (8 ports).
    // Only present when MADT was found (implies x86/APIC platform).
    #[cfg(target_arch = "x86_64")]
    if found_madt
    {
        push!(PlatformResource {
            resource_type: ResourceType::IoPortRange,
            flags: 0,
            base: 0xCF8,
            size: 8,
            id: 0,
        });
    }
    #[cfg(not(target_arch = "x86_64"))]
    let _ = found_madt;

    count
}

/// Find the UART base address from the ACPI SPCR table.
///
/// Returns `None` if RSDP is invalid, SPCR is not found, or the base address
/// space is not MMIO (address_space_id != 0).
///
/// # Safety
/// `rsdp_addr` must be a physical address of a valid, identity-mapped RSDP.
pub unsafe fn find_spcr_uart_base(rsdp_addr: u64) -> Option<u64>
{
    let rsdp = unsafe { phys_slice(rsdp_addr, 36) };
    if &rsdp[..8] != RSDP_SIG || read_u8(rsdp, RSDP_OFF_REVISION) < 2
    {
        return None;
    }
    let xsdt_addr = read_u64(rsdp, RSDP_OFF_XSDT);
    if xsdt_addr == 0
    {
        return None;
    }

    let xsdt_hdr = unsafe { phys_slice(xsdt_addr, SDT_HDR_LEN) };
    if &xsdt_hdr[SDT_OFF_SIGNATURE..SDT_OFF_SIGNATURE + 4] != b"XSDT"
    {
        return None;
    }
    let xsdt_len = read_u32(xsdt_hdr, SDT_OFF_LENGTH) as usize;
    if xsdt_len < SDT_HDR_LEN
    {
        return None;
    }
    let xsdt = unsafe { phys_slice(xsdt_addr, xsdt_len) };
    let entries_bytes = &xsdt[SDT_HDR_LEN..];
    let entry_count = entries_bytes.len() / 8;

    for i in 0..entry_count
    {
        let table_addr = read_u64(entries_bytes, i * 8);
        if table_addr == 0
        {
            continue;
        }
        let hdr = unsafe { phys_slice(table_addr, SDT_HDR_LEN) };
        let sig = &hdr[SDT_OFF_SIGNATURE..SDT_OFF_SIGNATURE + 4];
        if sig != b"SPCR"
        {
            continue;
        }
        let table_len = read_u32(hdr, SDT_OFF_LENGTH) as usize;
        if table_len < SPCR_GAS_ADDRESS + 8
        {
            return None;
        }
        let table = unsafe { phys_slice(table_addr, table_len) };
        let addr_space = read_u8(table, SPCR_GAS_ADDR_SPACE_ID);
        if addr_space != 0
        {
            // 0 = System Memory (MMIO). I/O port (1) not useful for our mapping.
            return None;
        }
        let uart_base = read_u64(table, SPCR_GAS_ADDRESS);
        if uart_base != 0
        {
            return Some(uart_base);
        }
    }

    None
}

// ── Private table parsers ─────────────────────────────────────────────────────

/// Parse a MADT table and append resources to `out[*count..]`.
///
/// Adds:
/// - Local APIC MmioRange (base from MADT header, size=4096)
/// - I/O APIC MmioRanges (one per type-1 entry)
/// - IRQ source overrides as IrqLine entries (one per type-2 entry)
fn parse_madt(table: &[u8], _table_addr: u64, count: &mut usize, out: &mut [PlatformResource])
{
    macro_rules! push {
        ($res:expr) => {
            if *count < out.len()
            {
                out[*count] = $res;
                *count += 1;
            }
        };
    }

    // Local APIC base address (u32 at offset 36).
    let lapic_base = read_u32(table, MADT_OFF_LAPIC_BASE) as u64;
    if lapic_base != 0
    {
        push!(PlatformResource {
            resource_type: ResourceType::MmioRange,
            flags: 0,
            base: lapic_base,
            size: 4096,
            id: 0,
        });
    }

    // Walk MADT interrupt controller structure entries.
    let mut off = MADT_ENTRIES_OFF;
    while off + 2 <= table.len()
    {
        let entry_type = read_u8(table, off);
        let entry_len = read_u8(table, off + 1) as usize;
        if entry_len < 2 || off + entry_len > table.len()
        {
            break;
        }

        match entry_type
        {
            MADT_TYPE_IOAPIC =>
            {
                // Type 1 (I/O APIC), total length = 12:
                //   off+0: type  off+1: length  off+2: io_apic_id  off+3: reserved
                //   off+4: io_apic_address(u32)  off+8: global_system_interrupt_base(u32)
                if entry_len >= 12
                {
                    let io_apic_id = read_u8(table, off + 2) as u64;
                    let io_apic_addr = read_u32(table, off + 4) as u64;
                    if io_apic_addr != 0
                    {
                        push!(PlatformResource {
                            resource_type: ResourceType::MmioRange,
                            flags: 0,
                            base: io_apic_addr,
                            size: 4096,
                            id: io_apic_id,
                        });
                    }
                }
            }
            MADT_TYPE_ISO =>
            {
                // Type 2 (Interrupt Source Override), total length = 10:
                //   off+0: type  off+1: length  off+2: bus  off+3: source
                //   off+4: global_system_interrupt(u32)  off+8: flags(u16)
                if entry_len >= 10
                {
                    let gsi = read_u32(table, off + 4) as u64;
                    let iso_flags = read_u16(table, off + 8);
                    // Map ACPI trigger/polarity flags to boot-protocol IrqLine flags:
                    //   trigger bits [3:2]: 3=level→bit0=0, else edge→bit0=1
                    //   polarity bits [1:0]: 3=active-low→bit1=1, else active-high→bit1=0
                    let trigger = (iso_flags >> 2) & 3;
                    let polarity = iso_flags & 3;
                    let resource_flags = if trigger == 3 { 0u32 } else { 1u32 }
                        | if polarity == 3 { 2u32 } else { 0u32 };
                    push!(PlatformResource {
                        resource_type: ResourceType::IrqLine,
                        flags: resource_flags,
                        base: 0,
                        size: 0,
                        id: gsi,
                    });
                }
            }
            _ =>
            {} // Skip unknown MADT entry types.
        }

        off += entry_len;
    }
}

/// Parse a MCFG table and append PciEcam entries to `out[*count..]`.
///
/// Each 16-byte MCFG entry maps a PCI segment group to an ECAM window.
fn parse_mcfg(table: &[u8], count: &mut usize, out: &mut [PlatformResource])
{
    macro_rules! push {
        ($res:expr) => {
            if *count < out.len()
            {
                out[*count] = $res;
                *count += 1;
            }
        };
    }

    let mut off = MCFG_ENTRIES_OFF;
    while off + MCFG_ENTRY_SIZE <= table.len()
    {
        // MCFG entry layout (16 bytes):
        //   0: base_address(u64)  8: pci_segment_group(u16)
        //  10: start_bus(u8)     11: end_bus(u8)    12-15: reserved
        let base = read_u64(table, off);
        let segment = read_u16(table, off + 8) as u64;
        let start_bus = read_u8(table, off + 10);
        let end_bus = read_u8(table, off + 11);

        if base != 0
        {
            let num_buses = (end_bus as u64).saturating_sub(start_bus as u64) + 1;
            // Each bus has 256 devices × 4096 bytes = 1 MiB per bus.
            let size = num_buses * 256 * 4096;
            let flags = (start_bus as u32) | ((end_bus as u32) << 8);
            push!(PlatformResource {
                resource_type: ResourceType::PciEcam,
                flags,
                base,
                size,
                id: segment,
            });
        }

        off += MCFG_ENTRY_SIZE;
    }
}
