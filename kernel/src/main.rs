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
//! - Phase 8: initialise per-CPU scheduler state and idle threads (BSP only; SMP in Phase 11).
//! - Phase 9: create init process address space + TCB; enter user mode.
//! - Phase 10: CSpace handoff to init; context switching + timer preemption; IPC syscalls.

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
mod ipc;
mod mm;
mod platform;
mod sched;
mod sync;
mod syscall;
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

    // ── Phase 9: create and launch init ───────────────────────────────────────
    // Gated #[cfg(not(test))]: Phase 9 uses heap allocation and arch-specific
    // functions unavailable in the host test environment. Tests exercise Phases
    // 0-8 via their individual stub functions; kernel_entry is never invoked.
    #[cfg(not(test))]
    {
        // Re-access BootInfo via the direct physical map (identity mapping no longer active).
        // SAFETY: boot_info holds the physical address of the BootInfo struct; after Phase 3
        // all physical memory is accessible via DIRECT_MAP_BASE.
        let info9 = unsafe {
            &*(mm::paging::phys_to_virt(boot_info as u64) as *const boot_protocol::BootInfo)
        };

        kprintln!("  phase 9: init image segments: {}", info9.init_image.segment_count);

        if info9.init_image.segment_count == 0 || info9.init_image.entry_point == 0
        {
            fatal("Phase 9: init image missing or has no entry point");
        }

        // Create init's user address space (PML4 / Sv48 root + kernel entries).
        // SAFETY: Phase 3 active, Phase 4 heap active; single-threaded boot.
        let init_as = unsafe { mm::address_space::AddressSpace::new_user(allocator) };
        let init_as_ptr = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(init_as));

        // Map each ELF LOAD segment into the init address space.
        for i in 0..info9.init_image.segment_count as usize
        {
            let seg = &info9.init_image.segments[i];
            // SAFETY: segment data is in Loaded memory reachable via the direct map.
            unsafe {
                (*init_as_ptr).map_segment(seg, allocator)
            }
            .unwrap_or_else(|_| fatal("Phase 9: failed to map init segment"));
        }

        // Map init's user stack (INIT_STACK_PAGES pages below INIT_STACK_TOP).
        // SAFETY: stack_top is page-aligned and within the user address range.
        unsafe {
            (*init_as_ptr).map_stack(
                mm::address_space::INIT_STACK_TOP,
                mm::address_space::INIT_STACK_PAGES,
                allocator,
            )
        }
        .unwrap_or_else(|_| fatal("Phase 9: failed to map init stack"));

        kprintln!("  phase 9: init address space ready");

        // Allocate init's kernel stack (KERNEL_STACK_PAGES = 4 pages = 16 KiB).
        let init_kstack_phys = allocator
            .alloc(2) // 2^2 = 4 pages
            .unwrap_or_else(|| fatal("Phase 9: out of memory for init kernel stack"));
        let init_kstack_virt = mm::paging::phys_to_virt(init_kstack_phys);
        let init_kstack_top =
            init_kstack_virt + (sched::KERNEL_STACK_PAGES * mm::PAGE_SIZE) as u64;

        // Build the init TCB.  saved_state.rip / .ra stores the user entry point
        // so sched::enter() can retrieve it without depending on BootInfo.
        let init_saved = arch::current::context::new_state(
            info9.init_image.entry_point,
            init_kstack_top,
            0, // arg (unused for user threads)
            true,
        );

        let init_tcb = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(
            sched::thread::ThreadControlBlock {
                state:           sched::thread::ThreadState::Ready,
                priority:        sched::INIT_PRIORITY,
                slice_remaining: sched::TIME_SLICE_TICKS,
                cpu_affinity:    sched::AFFINITY_ANY,
                preferred_cpu:   0,
                run_queue_next:  None,
                ipc_state:       sched::thread::IpcThreadState::None,
                ipc_msg:         ipc::message::Message::default(),
                reply_tcb:       core::ptr::null_mut(),
                ipc_wait_next:   None,
                is_user:         true,
                saved_state:     init_saved,
                kernel_stack_top: init_kstack_top,
                trap_frame:      core::ptr::null_mut(), // set in sched::enter()
                address_space:   init_as_ptr,
                ipc_buffer:      0,
                wakeup_value:    0,
                cspace:          {
                    // Transfer root CSpace ownership to init. ROOT_CSPACE is
                    // an Option<Box<CSpace>> set in Phase 7; take it here so
                    // the raw pointer is valid for the lifetime of the process.
                    // SAFETY: single-threaded boot; ROOT_CSPACE not yet accessed.
                    let cs = unsafe { cap::ROOT_CSPACE.take() }
                        .unwrap_or_else(|| fatal("Phase 9: ROOT_CSPACE missing"));
                    alloc::boxed::Box::into_raw(cs)
                },
                thread_id:       1, // 0 = idle BSP, 1 = init
            },
        ));

        // Enqueue init on the BSP scheduler at INIT_PRIORITY.
        // SAFETY: single-threaded boot; SCHEDULERS[0] is exclusively owned.
        unsafe {
            let sched = sched::scheduler_for(0);
            sched.enqueue(init_tcb, sched::INIT_PRIORITY);
        }

        kprintln!("  phase 9: init TCB enqueued (priority {})", sched::INIT_PRIORITY);

        // Hand off to the scheduler. Never returns.
        sched::enter();
    }

    // Test-mode divergence: kernel_entry is never called in host tests, but
    // the function must type-check as returning `!`.
    #[cfg(test)]
    arch::current::cpu::halt_loop()
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
