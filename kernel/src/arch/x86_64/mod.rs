// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/arch/x86_64/mod.rs

//! x86-64 architecture module for the kernel.

pub mod ap_trampoline;
pub mod console;
pub mod context;
pub mod cpu;
pub mod gdt;
pub mod idt;
pub mod interrupts;
pub mod ioapic;
pub mod paging;
pub mod syscall;
pub mod timer;
pub mod trap_frame;

/// Architecture name string for use in diagnostic output.
pub const ARCH_NAME: &str = "x86_64";

/// Maximum valid GSI (Global System Interrupt) number on x86-64.
/// I/O APIC delivers GSIs 0–255.
pub const MAX_IRQ_ID: u32 = 255;

/// Minimum valid GSI number on x86-64. GSI 0 (PIT timer) is a legitimate
/// platform resource; nothing is reserved at the low end.
pub const MIN_IRQ_ID: u32 = 0;

/// x86-64 has I/O port space; `IoPortRange` resources are valid here.
pub const HAS_IO_PORTS: bool = true;

/// Size of the I/O Permission Bitmap in bytes (re-exported from gdt for use
/// in architecture-independent code such as `ThreadControlBlock`).
pub use gdt::IOPB_SIZE;

/// MMIO regions that must be direct-mapped during Phase 3 page table setup.
///
/// Each entry is `(physical_base, size_in_bytes)`. These regions are mapped
/// at `DIRECT_MAP_BASE + phys` if they fall outside the RAM direct-map range.
/// - xAPIC local APIC register block: 4 KiB at `0xFEE0_0000.`
/// - I/O APIC register block: 4 KiB at `0xFEC0_0000` (standard Q35 base).
///
// TODO: Discover IOAPIC base from ACPI MADT rather than
// hardcoding. Pick up when ACPI table parsing is added.
pub const MMIO_DIRECT_MAP_REGIONS: &[(u64, u64)] = &[(0xFEE0_0000, 0x1000), (0xFEC0_0000, 0x1000)];
