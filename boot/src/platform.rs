// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// boot/src/platform.rs

//! Platform resource orchestrator.
//!
//! Architecture-neutral: calls the ACPI and/or DTB parsers based on what
//! firmware tables were discovered, allocates a physical page for the result
//! array, and sorts by `(resource_type, base)` before returning.
//!
//! Invoked as part of Step 5 (firmware discovery and platform resources),
//! before page table construction (Step 6), so the allocation can be included
//! in the identity-map budget.

use crate::acpi;
use crate::dtb;
use crate::error::BootError;
use crate::firmware::FirmwareInfo;
use crate::uefi::{allocate_pages, EfiBootServices};
use crate::{bprint, bprintln};
use boot_protocol::{PlatformResource, ResourceType};

/// Maximum number of platform resources tracked across all parsers.
///
/// Increase if ACPI + DTB together produce more than this many entries.
const MAX_PLATFORM_RESOURCES: usize = 64;

/// Parse platform firmware tables and return a physical allocation holding
/// the sorted [`PlatformResource`] array.
///
/// Returns `(physical_address, entry_count)`. If no resources are found,
/// returns `(0, 0)` (no allocation is made).
///
/// # Safety
/// `bs` must be valid UEFI boot services. `firmware` must contain physical
/// addresses from the UEFI configuration table discovered by `discover_firmware`.
pub unsafe fn parse_platform_resources(
    bs: *mut EfiBootServices,
    firmware: &FirmwareInfo,
) -> Result<(u64, usize), BootError>
{
    // Stack-allocate the scratch buffer. PlatformResource is Copy.
    let zero = PlatformResource {
        resource_type: ResourceType::MmioRange, // discriminant = 0
        flags: 0,
        base: 0,
        size: 0,
        id: 0,
    };
    let mut buf = [zero; MAX_PLATFORM_RESOURCES];
    let mut count = 0;

    // Parse ACPI tables (present on both x86-64 and RISC-V with EDK2).
    if firmware.acpi_rsdp != 0
    {
        // SAFETY: acpi_rsdp is a valid identity-mapped physical address from UEFI.
        let n = unsafe { acpi::parse_acpi_resources(firmware.acpi_rsdp, &mut buf[count..]) };
        count += n;
        bprint!("[--------] boot: ACPI: ");
        // cast_possible_truncation: n <= MAX_PLATFORM_RESOURCES (64), fits in u32.
        #[allow(clippy::cast_possible_truncation)]
        unsafe { crate::console::console_write_dec32(n as u32) };
        bprintln!(" resources");
    }

    // Parse Device Tree blob (present on RISC-V without ACPI, or alongside it).
    if firmware.device_tree != 0
    {
        // SAFETY: device_tree is a valid identity-mapped physical address from UEFI.
        let n = unsafe { dtb::parse_dtb_resources(firmware.device_tree, &mut buf[count..]) };
        count += n;
        bprint!("[--------] boot: DTB: ");
        // cast_possible_truncation: n <= MAX_PLATFORM_RESOURCES (64), fits in u32.
        #[allow(clippy::cast_possible_truncation)]
        unsafe { crate::console::console_write_dec32(n as u32) };
        bprintln!(" resources");
    }

    if count == 0
    {
        return Ok((0, 0));
    }

    // Sort by (resource_type as u32, base) ascending — insertion sort.
    // Using insertion sort avoids recursion (no stack growth from quicksort).
    let resources = &mut buf[..count];
    for i in 1..resources.len()
    {
        let mut j = i;
        while j > 0 && sort_key(&resources[j - 1]) > sort_key(&resources[j])
        {
            resources.swap(j - 1, j);
            j -= 1;
        }
    }

    // Allocate physical memory for the result array.
    let entry_size = core::mem::size_of::<PlatformResource>();
    let total_bytes = count * entry_size;
    let pages = total_bytes.div_ceil(4096);
    // SAFETY: bs is valid boot services.
    let phys = unsafe { allocate_pages(bs, pages)? };

    // Copy sorted entries into the allocated region.
    // SAFETY: phys is a fresh `pages`-page allocation; count entries fit within it.
    let ptr = phys as *mut PlatformResource;
    for (i, &r) in resources.iter().enumerate()
    {
        unsafe { core::ptr::write(ptr.add(i), r) };
    }

    Ok((phys, count))
}

/// Sort key: `(resource_type discriminant, base address)`.
#[inline]
fn sort_key(r: &PlatformResource) -> (u32, u64)
{
    (r.resource_type as u32, r.base)
}
