// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/main.rs

//! Seraph microkernel — kernel entry point.
//!
//! Receives control from the bootloader after page tables are installed and
//! UEFI boot services have exited. See `docs/boot-protocol.md` for the CPU
//! state contract and `BootInfo` layout.
//!
//! Initialization phases implemented here:
//! - Phase 0: validate `BootInfo` (pre-console; halts silently on failure).
//! - Phase 1: initialize early console (serial + framebuffer); emit startup banner.
//! - Phase 2: parse memory map, populate buddy frame allocator.
//! - Phase 3: install kernel page tables (direct physical map + W^X image).
//! - Phase 4: activate kernel heap (`GlobalAlloc` via slab/size-class allocator).
//! - Phase 5: architecture hardware init (GDT/IDT/APIC or stvec/PLIC, timer, syscall).
//! - Phase 6: validate `platform_resources` slice; reject malformed entries before capability minting.
//! - Phase 7: initialise capability subsystem; mint root CSpace with initial hardware caps.
//! - Phase 8: initialise per-CPU scheduler state and idle threads (BSP only; SMP in Phase 10).

#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
use core::panic::PanicInfo;

// Pull in the `alloc` crate (Box, Vec, …) for no_std kernel builds.
// In test mode the standard library provides alloc implicitly.
#[cfg(not(test))]
extern crate alloc;

use boot_protocol::BootInfo;

mod arch;
mod cap;
mod console;
mod framebuffer;
mod mm;
mod platform;
mod sched;
mod sync;
mod validate;

/// Kernel entry point.
///
/// Called by the bootloader with CPU state per `docs/boot-protocol.md`.
/// `boot_info` is the physical address of a populated [`BootInfo`] structure,
/// accessible before the kernel's own page tables are established because the
/// bootloader identity-maps the `BootInfo` region.
#[no_mangle]
pub extern "C" fn kernel_entry(boot_info: *const BootInfo) -> !
{
    // ── Phase 0: validate BootInfo ──────────────────────────────────────────
    // Pre-console. On failure the kernel halts silently; no output is possible
    // yet. GDB can distinguish this halt from a successful boot by checking
    // whether execution reaches the Phase 1 console init below.
    //
    // SAFETY: validate_boot_info checks null and alignment before dereferencing.
    if !unsafe { validate::validate_boot_info(boot_info) }
    {
        arch::current::cpu::halt_loop();
    }

    // SAFETY: validate_boot_info confirmed non-null, aligned, and readable.
    let info = unsafe { &*boot_info };

    // ── Phase 1: early console ──────────────────────────────────────────────
    // SAFETY: called exactly once, from the single kernel boot thread, after
    // Phase 0 confirmed boot_info is valid.
    unsafe {
        console::init(info);
    }

    kprintln!("Seraph kernel starting");
    kprintln!("  boot protocol version: {}", info.version);
    kprintln!("  architecture: {}", arch::current::ARCH_NAME);

    // ── Phase 2: physical memory ────────────────────────────────────────────
    // Parse the memory map, subtract reserved regions, populate the buddy
    // allocator. Halts with a FATAL message if no usable memory is found.
    //
    // SAFETY: single-threaded boot; FRAME_ALLOCATOR is not accessed elsewhere.
    let allocator = unsafe { &mut *core::ptr::addr_of_mut!(mm::FRAME_ALLOCATOR) };
    mm::init::init_physical_memory(info, allocator);
    kprintln!(
        "  usable RAM: {} MiB",
        allocator.free_page_count() * mm::PAGE_SIZE / (1024 * 1024)
    );

    // ── Phase 3: kernel page tables ─────────────────────────────────────────
    // Replace the bootloader's minimal page tables with the kernel's own,
    // establishing the direct physical map and W^X kernel image mappings.
    //
    // Save the framebuffer physical base before the switch; `info` is a
    // physical-address reference that is no longer identity-mapped in the
    // new tables (it is accessible via the direct map as a future Phase 4
    // concern). All further uses of `info` must be resolved before activate.
    let fb_phys = info.framebuffer.physical_base;
    if let Err(_e) = mm::paging::init_kernel_page_tables(info, allocator)
    {
        fatal("Phase 3: boot page table pool exhausted (RAM > 248 GiB?)");
    }
    // Rebase MMIO-based console devices to the direct physical map.
    // On RISC-V the UART is MMIO and must be accessed via the direct map after
    // the page table switch; on x86-64 the UART is I/O-mapped (no-op).
    // SAFETY: page tables are now active; direct map covers all physical RAM
    // and the UART MMIO region.
    unsafe {
        let uart_phys = arch::current::console::UART_PHYS_BASE;
        if uart_phys != 0
        {
            arch::current::console::rebase_serial(mm::paging::phys_to_virt(uart_phys));
        }
        console::rebase_framebuffer(fb_phys);
    }
    kprintln!(
        "  page tables: active (direct map at {:#x})",
        mm::paging::DIRECT_MAP_BASE
    );

    // ── Phase 4: kernel heap ─────────────────────────────────────────────────
    // Flip the GlobalAlloc ready flag. The SizeClassAllocator and KernelCaches
    // are const-initialised in their statics; no heap memory is consumed here.
    //
    // Note on bootloader page table frame reclamation:
    // Two categories of frames are NOT reclaimed:
    //   1. BOOT_TABLE_POOL (BSS array): part of the kernel image; cannot be
    //      freed to buddy. The unused portion (~750 KiB) is acceptable waste.
    //   2. Bootloader's original page tables: BootInfo does not record their
    //      physical addresses, so they cannot be identified. Future enhancement:
    //      have the bootloader pass old CR3/satp in BootInfo.
    mm::heap::init();
    kprintln!("  kernel heap active");

    // ── Phase 5: architecture hardware initialization ─────────────────────────
    kprintln!("  phase 5: hardware init");
    // SAFETY: single-threaded boot; heap active; direct map active.
    unsafe {
        arch::current::interrupts::init();
    }
    kprintln!("  interrupts: initialized");
    // Enable preemption timer at 10 ms period.
    // timer::init() enables interrupts as its final step.
    unsafe {
        arch::current::timer::init(10_000);
    }
    kprintln!("  timer: running");
    unsafe {
        arch::current::syscall::init();
    }
    kprintln!("  syscall: entry installed");
    kprintln!("  phase 5: complete");

    // ── Phase 6: platform resource validation ─────────────────────────────────
    // Validate platform_resources from BootInfo before Phase 7 mints
    // capabilities from them. Returns only valid, non-overlapping entries.
    let platform_resources = platform::validate_platform_resources(boot_info as u64);

    // ── Phase 7: capability system ─────────────────────────────────────────────
    // Initialises the root CSpace and mints initial capabilities for all
    // boot-provided hardware resources.
    let cap_count = cap::init_capability_system(platform_resources, boot_info as u64);
    kprintln!("  capability system: {} slots populated", cap_count);

    // ── Phase 8: scheduler ────────────────────────────────────────────────────
    // Initialise per-CPU scheduler state and create idle threads.
    // BSP only (cpu_count = 1); SMP bringup in Phase 10.
    //
    // SAFETY: single-threaded boot; heap and page tables are active.
    let cpu_count = sched::init(1, allocator);
    kprintln!("  scheduler: initialised, {} CPU", cpu_count);

    // ── TODO: Phase 9+ ────────────────────────────────────────────────────────
    arch::current::cpu::halt_loop();
}

/// Emit a fatal error message and halt.
///
/// Used for unrecoverable post-console errors. Prints the message then halts
/// permanently. Never returns.
pub(crate) fn fatal(msg: &str) -> !
{
    kprintln!("FATAL: {}", msg);
    arch::current::cpu::halt_loop();
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> !
{
    arch::current::cpu::halt_loop();
}
