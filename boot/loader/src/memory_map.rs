// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// boot/loader/src/memory_map.rs

//! Memory map translation helpers.
//!
//! Converts the UEFI memory descriptor list into the boot protocol's
//! `MemoryMapEntry` format and sorts the result by physical base address.

use crate::uefi::{
    EfiMemoryDescriptor, EFI_ACPI_MEMORY_NVS, EFI_ACPI_RECLAIM_MEMORY, EFI_BOOT_SERVICES_CODE,
    EFI_BOOT_SERVICES_DATA, EFI_CONVENTIONAL_MEMORY, EFI_LOADER_CODE, EFI_LOADER_DATA,
    EFI_MEMORY_MAPPED_IO, EFI_MEMORY_MAPPED_IO_PORT_SPACE, EFI_PERSISTENT_MEMORY,
    EFI_RUNTIME_SERVICES_CODE, EFI_RUNTIME_SERVICES_DATA,
};
use boot_protocol::{MemoryMapEntry, MemoryType};

/// Translate a UEFI memory map into the boot protocol's `MemoryMapEntry` format.
///
/// Iterates `uefi_map.map_size / uefi_map.descriptor_size` descriptors and
/// converts each UEFI memory type to a [`MemoryType`]. Writes at most
/// `max_entries` entries to `out`. Returns the number of entries written.
///
/// # Safety
/// `out` must point to an allocation of at least `max_entries * size_of::<MemoryMapEntry>()`
/// bytes. `uefi_map.buffer_phys` must be the UEFI raw map buffer from the most
/// recent `GetMemoryMap` call, with valid descriptors.
pub unsafe fn translate_memory_map(
    uefi_map: &crate::uefi::MemoryMapResult,
    out: *mut MemoryMapEntry,
    max_entries: usize,
) -> usize
{
    let mut count: usize = 0;
    let mut offset: usize = 0;

    while offset + uefi_map.descriptor_size <= uefi_map.map_size && count < max_entries
    {
        // SAFETY: uefi_map.buffer_phys is the UEFI raw map buffer; offset is
        // within map_size; the descriptor at this offset is a valid EfiMemoryDescriptor.
        let desc =
            unsafe { &*((uefi_map.buffer_phys as usize + offset) as *const EfiMemoryDescriptor) };

        let memory_type = translate_memory_type(desc.memory_type);
        let size_bytes = desc.number_of_pages * 4096;

        // SAFETY: count < max_entries; out[count] is within the allocated array.
        unsafe {
            core::ptr::write(
                out.add(count),
                MemoryMapEntry {
                    physical_base: desc.physical_start,
                    size: size_bytes,
                    memory_type,
                },
            );
        }
        count += 1;
        offset += uefi_map.descriptor_size;
    }

    count
}

/// Translate a UEFI `EFI_MEMORY_TYPE` value to the boot protocol's [`MemoryType`].
///
/// Per `boot/docs/uefi-environment.md`:
/// - `EfiConventionalMemory`, `EfiBootServicesCode/Data` → `Usable` (reclaimable)
/// - `EfiLoaderCode/Data` → `Loaded` (in-use by bootloader)
/// - `EfiACPIReclaimMemory` → `AcpiReclaimable`
/// - `EfiPersistentMemory` → `Persistent`
/// - All other types → `Reserved`
fn translate_memory_type(uefi_type: u32) -> MemoryType
{
    match uefi_type
    {
        EFI_CONVENTIONAL_MEMORY | EFI_BOOT_SERVICES_CODE | EFI_BOOT_SERVICES_DATA =>
        {
            MemoryType::Usable
        }

        EFI_LOADER_CODE | EFI_LOADER_DATA => MemoryType::Loaded,

        EFI_ACPI_RECLAIM_MEMORY => MemoryType::AcpiReclaimable,

        EFI_PERSISTENT_MEMORY => MemoryType::Persistent,

        // EfiRuntimeServicesCode/Data, EfiACPIMemoryNVS, EfiMemoryMappedIO,
        // EfiMemoryMappedIOPortSpace, and all unrecognised types are Reserved.
        EFI_RUNTIME_SERVICES_CODE
        | EFI_RUNTIME_SERVICES_DATA
        | EFI_ACPI_MEMORY_NVS
        | EFI_MEMORY_MAPPED_IO
        | EFI_MEMORY_MAPPED_IO_PORT_SPACE => MemoryType::Reserved,

        _ => MemoryType::Reserved,
    }
}

/// Sort `MemoryMapEntry` elements in `[0..count)` by `physical_base` ascending.
///
/// Uses insertion sort — O(n²) worst case, but n is always small (< 700) and
/// the UEFI memory map is often already nearly sorted by the firmware.
///
/// # Safety
/// `entries` must point to an allocation of at least `count` valid, initialised
/// `MemoryMapEntry` elements.
pub unsafe fn insertion_sort_memory_map(entries: *mut MemoryMapEntry, count: usize)
{
    for i in 1..count
    {
        // SAFETY: i < count; entries[i] is a valid initialised MemoryMapEntry.
        let key = unsafe { core::ptr::read(entries.add(i)) };
        let mut j = i;

        while j > 0
        {
            // SAFETY: j - 1 < i < count; entries[j-1] is initialised.
            let prev_base = unsafe { (*entries.add(j - 1)).physical_base };
            if prev_base <= key.physical_base
            {
                break;
            }
            // SAFETY: j and j-1 are both < count and within the allocation.
            unsafe { core::ptr::copy_nonoverlapping(entries.add(j - 1), entries.add(j), 1) };
            j -= 1;
        }

        // SAFETY: j < count; entries[j] is within the allocation.
        unsafe { core::ptr::write(entries.add(j), key) };
    }
}
