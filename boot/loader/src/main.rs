// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// boot/loader/src/main.rs

//! Seraph UEFI bootloader — ten-step boot sequence orchestrator.
//!
//! Loads the kernel ELF and init module from the ESP, establishes initial
//! page tables with W^X enforcement, discovers firmware table addresses,
//! exits UEFI boot services, populates `BootInfo`, and jumps to the kernel
//! entry point. See `boot/docs/boot-flow.md` for the step-by-step design.

#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]
#![feature(never_type)]

// Include the hand-crafted PE/COFF header for RISC-V UEFI builds. LLVM has no
// PE/COFF backend for RISC-V, so we prepend this header and convert with
// llvm-objcopy. See boot/loader/src/arch/riscv64/header.S and
// boot/loader/linker/riscv64-uefi.ld.
#[cfg(target_arch = "riscv64")]
core::arch::global_asm!(include_str!("arch/riscv64/header.S"));

mod arch;
mod config;
mod console;
mod elf;
mod error;
mod firmware;
mod font;
mod framebuffer;
mod memory_map;
mod paging;
mod uefi;

use crate::config::load_boot_config;
use crate::elf::{load_kernel, load_module};
use crate::error::BootError;
use crate::firmware::discover_firmware;
use crate::paging::{build_initial_tables, PageTableBuilder};
use crate::uefi::{
    allocate_pages, connect_all_controllers, exit_boot_services, file_read, file_size,
    get_loaded_image, get_memory_map, open_esp_volume, open_file, query_gop, EfiHandle,
    EfiSystemTable,
};
use boot_protocol::{
    BootInfo, BootModule, MemoryMapEntry, MemoryMapSlice, ModuleSlice, PlatformResourceSlice,
    BOOT_PROTOCOL_VERSION,
};

// ── Size constants ────────────────────────────────────────────────────────────

/// Number of 4 KiB pages allocated for the kernel stack (64 KiB).
const KERNEL_STACK_PAGES: usize = 16;

/// Number of 4 KiB pages allocated for the translated `MemoryMapEntry` output
/// array. At 24 bytes per entry this accommodates roughly 680 entries, which
/// comfortably exceeds any real UEFI memory map.
const MEM_MAP_ENTRY_PAGES: usize = 4;

/// Maximum number of physical regions tracked for identity mapping.
/// Covers kernel segments, all fixed allocations, and the framebuffer.
const MAX_IDENTITY_REGIONS: usize = 64;

// ── Entry point ───────────────────────────────────────────────────────────────

/// UEFI application entry point.
///
/// UEFI firmware calls this function after loading and relocating the
/// bootloader image. Delegates immediately to [`boot_sequence`] and prints
/// a fatal error message before halting if the sequence fails.
///
/// Returns a `usize` (UEFI `EFI_STATUS`) to satisfy the UEFI ABI, but in
/// practice never returns — the boot sequence either jumps to the kernel or
/// halts on error.
#[no_mangle]
pub extern "efiapi" fn efi_main(image: EfiHandle, st: *mut EfiSystemTable) -> usize
{
    // SAFETY: serial_init called exactly once, before boot_sequence.
    unsafe {
        crate::console::init_serial();
    }

    match unsafe { boot_sequence(image, st) }
    {
        Ok(never) => match never {},
        Err(err) => error::fatal_error(&err),
    }
}

// ── Boot sequence ─────────────────────────────────────────────────────────────

/// Execute the ten-step boot sequence and transfer control to the kernel.
///
/// Returns `Result<!, BootError>`: the `Ok` variant is uninhabited (`!`) because
/// a successful sequence ends with a non-returning kernel jump. Any `Err` is
/// propagated to `efi_main` for error reporting.
///
/// # Safety
/// `image` must be the UEFI image handle passed to `efi_main`. `st` must be
/// a valid pointer to the UEFI system table.
unsafe fn boot_sequence(image: EfiHandle, st: *mut EfiSystemTable) -> Result<!, BootError>
{
    // SAFETY: st is validated by the caller (efi_main receives it from UEFI).
    let bs = unsafe { (*st).boot_services };

    // ── Step 1: UEFI protocol discovery ──────────────────────────────────────

    bprintln!("seraph-boot: step 1/10: UEFI protocol discovery");

    // SAFETY: bs is valid boot services; image is the EFI application handle.
    let loaded_image = unsafe { get_loaded_image(bs, image)? };
    // SAFETY: loaded_image is a valid EFI_LOADED_IMAGE_PROTOCOL pointer.
    let device_handle = unsafe { (*loaded_image).device_handle };
    // SAFETY: bs is valid; device_handle is the boot volume device handle.
    let esp_root = unsafe { open_esp_volume(bs, device_handle)? };
    // Force EDK2 to bind device drivers (e.g. virtio-gpu → GOP) on platforms
    // that don't auto-connect during BDS (notably RISC-V).
    // SAFETY: bs is valid boot services.
    unsafe {
        connect_all_controllers(bs);
    }

    // GOP is optional; absence is handled gracefully with a zeroed FramebufferInfo.
    // SAFETY: bs is valid.
    let framebuffer =
        unsafe { query_gop(bs) }.unwrap_or_else(boot_protocol::FramebufferInfo::empty);

    // SAFETY: framebuffer describes a valid GOP framebuffer (or is zeroed if absent).
    unsafe {
        crate::console::init_framebuffer(&framebuffer);
    }

    if framebuffer.physical_base != 0
    {
        bprintln!("seraph-boot:   GOP: present");
    }
    else
    {
        bprintln!("seraph-boot:   GOP: absent (headless)");
    }

    // ── Step 2: Load boot configuration ──────────────────────────────────────

    bprintln!("seraph-boot: step 2/10: load boot configuration");

    // SAFETY: esp_root is a valid EFI_FILE_PROTOCOL directory handle.
    let config = unsafe { load_boot_config(esp_root)? };

    // ── Step 3: Load kernel ELF ───────────────────────────────────────────────

    bprintln!("seraph-boot: step 3/10: loading kernel ELF");

    // SAFETY: esp_root is a valid directory handle; path is a null-terminated UTF-16.
    let kernel_file =
        unsafe { open_file(esp_root, config.kernel_path.as_ptr(), "kernel (boot.conf)")? };
    // SAFETY: kernel_file is a valid open file handle.
    let kernel_file_sz = unsafe { file_size(kernel_file)? } as usize;
    let kernel_buf_pages = (kernel_file_sz + 4095) / 4096;
    // SAFETY: bs is valid.
    let kernel_buf_phys = unsafe { allocate_pages(bs, kernel_buf_pages)? };
    // SAFETY: kernel_buf_phys is a freshly allocated region of kernel_buf_pages*4096 bytes,
    // identity-mapped by UEFI. Slicing to kernel_file_sz is within the allocation.
    let kernel_buf =
        unsafe { core::slice::from_raw_parts_mut(kernel_buf_phys as *mut u8, kernel_file_sz) };
    // SAFETY: kernel_file is open and at position 0; kernel_buf is the correct size.
    unsafe { file_read(kernel_file, kernel_buf)? };
    // SAFETY: bs is valid; kernel_buf is the complete ELF file.
    let kernel_info = unsafe { load_kernel(bs, kernel_buf, arch::current::EXPECTED_ELF_MACHINE)? };

    // Use direct-write helpers instead of format-arg bprintln! to avoid vtable
    // dispatch. On RISC-V, the PE .reloc section is currently empty, so the
    // firmware does not patch vtable entries when relocating the image;
    // core::fmt::write's write_str call through a fat pointer faults.
    bprint!("seraph-boot:   kernel entry=");
    unsafe {
        crate::console::console_write_hex64(kernel_info.entry_virtual);
    }
    bprint!("  size=");
    unsafe {
        crate::console::console_write_hex64(kernel_info.size as u64);
    }
    bprintln!(" bytes");

    // ── Step 4: Load init module ──────────────────────────────────────────────

    bprintln!("seraph-boot: step 4/10: loading init module");

    // SAFETY: esp_root is a valid directory handle; path is null-terminated UTF-16.
    let init_file = unsafe { open_file(esp_root, config.init_path.as_ptr(), "init (boot.conf)")? };
    // SAFETY: init_file is a valid open file handle.
    let init_file_sz = unsafe { file_size(init_file)? } as usize;
    let init_buf_pages = (init_file_sz + 4095) / 4096;
    // SAFETY: bs is valid.
    let init_buf_phys = unsafe { allocate_pages(bs, init_buf_pages)? };
    // SAFETY: init_buf_phys is a freshly allocated region; slice is within the allocation.
    let init_buf =
        unsafe { core::slice::from_raw_parts_mut(init_buf_phys as *mut u8, init_file_sz) };
    // SAFETY: init_file is open at position 0; init_buf is the correct size.
    unsafe { file_read(init_file, init_buf)? };
    // SAFETY: bs is valid; init_buf contains the complete module file.
    let init_module: BootModule = unsafe { load_module(bs, init_buf)? };

    // ── Step 5: Firmware discovery ────────────────────────────────────────────

    bprintln!("seraph-boot: step 5/10: firmware discovery");

    // Scan the UEFI configuration table for the ACPI RSDP and Device Tree GUIDs.
    // SAFETY: st is a valid UEFI system table pointer.
    let firmware = unsafe { discover_firmware(st) };

    if firmware.acpi_rsdp != 0
    {
        bprintln!("seraph-boot:   ACPI RSDP: found");
    }
    else
    {
        bprintln!("seraph-boot:   ACPI RSDP: not found");
    }
    if firmware.device_tree != 0
    {
        bprintln!("seraph-boot:   DTB: found");
    }
    else
    {
        bprintln!("seraph-boot:   DTB: not found");
    }

    // ── Step 6: Pre-allocate boot structures and build page tables ────────────

    // BootInfo: one page — holds the BootInfo struct populated in step 8.
    // SAFETY: bs is valid.
    let boot_info_phys = unsafe { allocate_pages(bs, 1)? };

    // Module array: one page — holds the [BootModule; 1] array for BootInfo.modules.
    // SAFETY: bs is valid.
    let modules_phys = unsafe { allocate_pages(bs, 1)? };

    // MemoryMapEntry output array: MEM_MAP_ENTRY_PAGES pages — the translated
    // physical memory map. Passed to BootInfo; must remain accessible to the kernel.
    // SAFETY: bs is valid.
    let mem_entries_phys = unsafe { allocate_pages(bs, MEM_MAP_ENTRY_PAGES)? };

    // Kernel stack: KERNEL_STACK_PAGES pages (64 KiB). Stack grows downward;
    // stack_top is the address of the byte past the last allocated page.
    // SAFETY: bs is valid.
    let stack_phys = unsafe { allocate_pages(bs, KERNEL_STACK_PAGES)? };
    let stack_top = stack_phys + (KERNEL_STACK_PAGES as u64) * 4096;

    // Command line: one page — contains a single null byte (empty command line).
    // SAFETY: bs is valid.
    let cmdline_phys = unsafe { allocate_pages(bs, 1)? };
    // SAFETY: cmdline_phys is a valid 4096-byte allocation; we write a single null byte.
    unsafe { core::ptr::write(cmdline_phys as *mut u8, 0u8) };

    // Accumulate all physical regions that must be identity-mapped so the kernel
    // can access them before its own page tables are established. The array is
    // fixed-size; region_count tracks valid entries.
    let mut identity_regions: [(u64, u64); MAX_IDENTITY_REGIONS] =
        [(0u64, 0u64); MAX_IDENTITY_REGIONS];
    let mut region_count: usize = 0;

    /// Add a (physical_base, size) pair to the identity-map tracking array.
    /// Silently drops entries beyond MAX_IDENTITY_REGIONS (design budget is 64).
    macro_rules! track_region {
        ($phys:expr, $size:expr) => {
            if region_count < MAX_IDENTITY_REGIONS
            {
                identity_regions[region_count] = ($phys as u64, $size as u64);
                region_count += 1;
            }
        };
    }

    // Kernel ELF segments: each mapped at its ELF virtual address by the page
    // table builder, but the physical range must also be identity-mapped so the
    // kernel can verify the mapping before establishing its own tables.
    for i in 0..kernel_info.segment_count
    {
        let seg = &kernel_info.segments[i];
        // Align size up to page boundary for identity-map coverage.
        let aligned_size = (seg.size + 4095) & !4095;
        track_region!(seg.phys_base, aligned_size);
    }

    // Fixed boot allocations — all must be identity-mapped.
    track_region!(boot_info_phys, 4096u64);
    track_region!(modules_phys, 4096u64);
    track_region!(mem_entries_phys, (MEM_MAP_ENTRY_PAGES as u64) * 4096);
    track_region!(stack_phys, (KERNEL_STACK_PAGES as u64) * 4096);
    track_region!(cmdline_phys, 4096u64);
    track_region!(init_module.physical_base, (init_module.size + 4095) & !4095);
    track_region!(kernel_buf_phys, (kernel_buf_pages as u64) * 4096);
    track_region!(init_buf_phys, (init_buf_pages as u64) * 4096);

    // Framebuffer: identity-map if present so the kernel can write early output.
    if framebuffer.physical_base != 0
    {
        let fb_size = (framebuffer.stride as u64) * (framebuffer.height as u64);
        track_region!(framebuffer.physical_base, (fb_size + 4095) & !4095);
    }

    bprintln!("seraph-boot: step 6/10: building page tables");

    // Build the initial page tables. All AllocatePages calls for page table
    // frames happen here, before ExitBootServices.
    let mut page_table =
        build_initial_tables(bs, &kernel_info, &identity_regions[0..region_count])?;

    // Identity-map the handoff trampoline page(s) as RX in the new page tables.
    // After `mov cr3` the CPU fetches the next instruction at the same VA; that
    // VA must be mapped RX in the new tables or the CPU faults immediately.
    // Under UEFI x86-64, VA == PA, so we use the symbol address as the physical
    // address directly. On RISC-V the stub returns (0,0); we skip mapping.
    let (tramp_first, tramp_last) = arch::current::trampoline_page_range();
    if tramp_first != 0
    {
        let tramp_flags = paging::PageFlags {
            writable: false,
            executable: true,
        };
        page_table
            .map(tramp_first, tramp_first, 4096, tramp_flags)
            .map_err(|e| match e
            {
                paging::MapError::OutOfMemory => BootError::OutOfMemory,
                paging::MapError::WxViolation => BootError::WxViolation,
            })?;
        if tramp_last != tramp_first
        {
            let tramp_flags = paging::PageFlags {
                writable: false,
                executable: true,
            };
            page_table
                .map(tramp_last, tramp_last, 4096, tramp_flags)
                .map_err(|e| match e
                {
                    paging::MapError::OutOfMemory => BootError::OutOfMemory,
                    paging::MapError::WxViolation => BootError::WxViolation,
                })?;
        }
    }

    // ── Step 7: Query final memory map ────────────────────────────────────────

    bprintln!("seraph-boot: step 7/10: querying memory map");

    // This must be the last allocation-generating operation before ExitBootServices.
    // AllocatePages inside get_memory_map invalidates any prior map key; this call
    // produces the map key used in step 7.
    // SAFETY: bs is valid.
    let mut uefi_map = unsafe { get_memory_map(bs)? };

    // ── Step 8: ExitBootServices ──────────────────────────────────────────────

    bprintln!("seraph-boot: step 8/10: ExitBootServices");

    // SAFETY: bs and image are valid; uefi_map was produced by the preceding
    // get_memory_map call with no intervening allocations.
    unsafe { exit_boot_services(bs, image, &mut uefi_map)? };

    // ══════════════════════════════════════════════════════════════════════════
    // NO UEFI CALLS AFTER THIS POINT. Boot services are permanently unavailable.
    // All subsequent operations use only pre-allocated physical memory.
    // ══════════════════════════════════════════════════════════════════════════

    // ── Step 9: Populate BootInfo ─────────────────────────────────────────────

    bprintln!("seraph-boot: step 9/10: populating BootInfo");

    // Write the module array to its pre-allocated page.
    // SAFETY: modules_phys is a valid 4096-byte physical allocation, identity-mapped
    // by UEFI. Writing one BootModule struct is within the allocation.
    unsafe { core::ptr::write(modules_phys as *mut BootModule, init_module) };

    // Translate the UEFI memory descriptors into the boot protocol's MemoryMapEntry
    // format and sort by physical_base ascending.
    let entry_out = mem_entries_phys as *mut MemoryMapEntry;
    let max_entries = (MEM_MAP_ENTRY_PAGES * 4096) / core::mem::size_of::<MemoryMapEntry>();
    let entry_count = memory_map::translate_memory_map(&uefi_map, entry_out, max_entries);

    // Sort the output array by physical_base ascending (insertion sort).
    // SAFETY: entry_out is a valid allocated array of max_entries entries;
    // elements [0..entry_count] were written by translate_memory_map above.
    unsafe { memory_map::insertion_sort_memory_map(entry_out, entry_count) };

    // Write a populated BootInfo into the pre-allocated page.
    // SAFETY: boot_info_phys is a valid 4096-byte physical allocation, identity-mapped
    // by UEFI. BootInfo fits comfortably within one page.
    unsafe {
        core::ptr::write(
            boot_info_phys as *mut BootInfo,
            BootInfo {
                version: BOOT_PROTOCOL_VERSION,
                memory_map: MemoryMapSlice {
                    entries: entry_out,
                    count: entry_count as u64,
                },
                kernel_physical_base: kernel_info.physical_base,
                kernel_virtual_base: kernel_info.virtual_base,
                kernel_size: kernel_info.size,
                modules: ModuleSlice {
                    entries: modules_phys as *const BootModule,
                    count: 1,
                },
                framebuffer,
                acpi_rsdp: firmware.acpi_rsdp,
                device_tree: firmware.device_tree,
                platform_resources: PlatformResourceSlice {
                    // TODO: Populate platform_resources with parsed ACPI/DT platform
                    // data. Requires an ACPI table parser and/or DTB parser to extract
                    // interrupt controller, timer, and device information. Deferred
                    // until the kernel's device management subsystem is ready to
                    // consume this data. See docs/device-management.md.
                    entries: core::ptr::null(),
                    count: 0,
                },
                command_line: cmdline_phys as *const u8,
                command_line_len: 0,
            },
        )
    };

    // ── Step 10: Kernel handoff ───────────────────────────────────────────────

    bprintln!("seraph-boot: step 10/10: handoff to kernel");

    // Transfer control to the kernel. Installs new page tables, sets the
    // stack, and jumps to the kernel entry point. Does not return.
    // SAFETY: page_table was built from valid physical frames covering all
    // required virtual ranges. entry_virtual is within the loaded kernel image.
    // boot_info_phys is a populated BootInfo in a pre-allocated, identity-mapped
    // page. stack_top is the top of a 64 KiB pre-allocated, identity-mapped
    // stack region. ExitBootServices has completed.
    unsafe {
        arch::current::perform_handoff(
            page_table.root_physical(),
            kernel_info.entry_virtual,
            boot_info_phys,
            stack_top,
        )
    }
}
