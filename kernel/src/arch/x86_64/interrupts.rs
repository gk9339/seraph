// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/arch/x86_64/interrupts.rs

//! x86-64 interrupt controller (xAPIC) and Phase 5 interrupt initialisation.
//!
//! Orchestrates GDT, IDT, SMEP/SMAP, and the local APIC in the correct order:
//!
//! 1. Enable SMEP + SMAP (fatal if CPU lacks support).
//! 2. Allocate IST stacks (2 × 8 KiB) from the heap via `Box::leak`.
//! 3. Load GDT + TSS with the IST stack pointers.
//! 4. Load IDT.
//! 5. Software-enable the local APIC: set SVR bit 8, spurious vector 255.
//! 6. Mask all LVT entries.
//!
//! Interrupts are **not** enabled here; `timer::init()` enables them after the
//! APIC timer is calibrated and configured.
//!
//! # xAPIC layout
//! The local APIC is accessed at physical address `0xFEE0_0000`, which is
//! accessible via the kernel's direct physical map at `DIRECT_MAP_BASE + phys`.
//!
//! # Modification notes
//! - To handle a new device IRQ: call `register_handler(vec, handler)` and
//!   call `unmask(vec)`. Full routing is deferred to a later phase.
//! - To migrate to x2APIC: replace `apic_read`/`apic_write` with MSR accesses
//!   (IA32_X2APIC_*) and update the SVR/LVT offsets.

#[cfg(not(test))]
extern crate alloc;
#[cfg(not(test))]
use alloc::boxed::Box;
#[cfg(not(test))]
use alloc::vec;

#[cfg(not(test))]
use super::{cpu, gdt, idt};
#[cfg(not(test))]
use crate::mm::paging::DIRECT_MAP_BASE;

// ── xAPIC constants ───────────────────────────────────────────────────────────

/// Physical base of the memory-mapped local APIC registers.
const APIC_BASE_PHYS: u64 = 0xFEE0_0000;

/// Spurious Interrupt Vector Register offset.
const APIC_SVR: usize = 0xF0;
/// End-of-Interrupt register offset (write 0 to acknowledge).
const APIC_EOI: usize = 0xB0;
/// LVT Timer register offset.
const APIC_LVT_TIMER: usize = 0x320;
/// LVT LINT0 register offset.
const APIC_LVT_LINT0: usize = 0x350;
/// LVT LINT1 register offset.
const APIC_LVT_LINT1: usize = 0x360;
/// LVT Error register offset.
const APIC_LVT_ERROR: usize = 0x370;
/// LVT Thermal monitor register offset.
const APIC_LVT_THERMAL: usize = 0x330;
/// LVT Performance counter register offset.
const APIC_LVT_PERF: usize = 0x340;

/// Bit to mask an LVT entry (prevent delivery).
const LVT_MASK: u32 = 1 << 16;

/// IST stack size: 8 KiB.
const IST_STACK_SIZE: usize = 8192;

// ── APIC register access ──────────────────────────────────────────────────────

/// Write `val` to APIC register at `offset` bytes from the APIC base.
///
/// # Safety
/// Must only be called after Phase 3 (direct map active) and with a valid
/// APIC register offset.
#[cfg(not(test))]
unsafe fn apic_write(offset: usize, val: u32)
{
    let vaddr = (DIRECT_MAP_BASE + APIC_BASE_PHYS) as usize + offset;
    // SAFETY: vaddr is within the direct-mapped APIC MMIO region.
    unsafe {
        core::ptr::write_volatile(vaddr as *mut u32, val);
    }
}

/// Read an APIC register at `offset` bytes from the APIC base.
#[cfg(not(test))]
fn apic_read(offset: usize) -> u32
{
    let vaddr = (DIRECT_MAP_BASE + APIC_BASE_PHYS) as usize + offset;
    // SAFETY: vaddr is within the direct-mapped APIC MMIO region.
    unsafe { core::ptr::read_volatile(vaddr as *const u32) }
}

// ── Public interface ──────────────────────────────────────────────────────────

/// Initialise interrupt infrastructure for x86-64.
///
/// Must be called once during Phase 5, after the heap is active (Phase 4)
/// and before `timer::init()`.
///
/// # Safety
/// Must execute at ring 0 from a single-threaded context.
#[cfg(not(test))]
pub unsafe fn init()
{
    // 1. Enable SMEP + SMAP — fatal if CPU lacks support.
    // SAFETY: ring-0 single-threaded boot.
    unsafe {
        cpu::enable_smep_smap();
    }

    // 2. Allocate IST stacks from the heap.
    // 8 KiB each, leaked so they live for the rest of kernel execution.
    let ist1_stack = Box::leak(vec![0u8; IST_STACK_SIZE].into_boxed_slice());
    let ist2_stack = Box::leak(vec![0u8; IST_STACK_SIZE].into_boxed_slice());
    let ist1_top = ist1_stack.as_ptr() as u64 + IST_STACK_SIZE as u64;
    let ist2_top = ist2_stack.as_ptr() as u64 + IST_STACK_SIZE as u64;

    // Derive the current kernel stack top from RSP for the initial TSS RSP0.
    // This is updated on each context switch in later phases.
    let rsp0: u64;
    // SAFETY: RSP is always readable at ring 0.
    unsafe {
        core::arch::asm!("mov {}, rsp", out(reg) rsp0, options(nostack, nomem));
    }

    // 3. Load GDT + TSS.
    // SAFETY: single-threaded boot; IST stacks just allocated.
    unsafe {
        gdt::init(rsp0, ist1_top, ist2_top);
    }

    // 4. Load IDT.
    // SAFETY: GDT is loaded; KERNEL_CS selector is valid.
    unsafe {
        idt::init();
    }

    // 5. Software-enable the local APIC.
    // Set SVR bit 8 (APIC Software Enable) and program spurious vector 255.
    // SAFETY: direct map is active; APIC MMIO is accessible.
    unsafe {
        apic_write(APIC_SVR, apic_read(APIC_SVR) | 0x100 | 0xFF);
    }

    // 6. Mask all LVT entries to prevent unexpected interrupts before the
    //    timer is configured.
    unsafe {
        apic_write(APIC_LVT_TIMER, LVT_MASK);
        apic_write(APIC_LVT_LINT0, LVT_MASK);
        apic_write(APIC_LVT_LINT1, LVT_MASK);
        apic_write(APIC_LVT_ERROR, LVT_MASK);
        apic_write(APIC_LVT_THERMAL, LVT_MASK);
        apic_write(APIC_LVT_PERF, LVT_MASK);
    }
}

/// No-op test stub: interrupt initialisation cannot run in host unit tests.
#[cfg(test)]
pub unsafe fn init() {}

/// Disable interrupts and return the previous IF state.
///
/// Returns `true` if interrupts were enabled before the call.
pub fn disable() -> bool
{
    let rflags: u64;
    // SAFETY: reads RFLAGS, then cli.
    unsafe {
        core::arch::asm!(
            "pushfq",
            "pop {0}",
            "cli",
            out(reg) rflags,
            options(nostack),
        );
    }
    rflags & (1 << 9) != 0 // IF is bit 9
}

/// Enable interrupts.
///
/// # Safety
/// IDT must be loaded before calling this function.
pub unsafe fn enable()
{
    // SAFETY: caller guarantees IDT is valid.
    unsafe {
        core::arch::asm!("sti", options(nostack, nomem));
    }
}

/// Return `true` if the interrupt flag (IF) is set in RFLAGS.
pub fn are_enabled() -> bool
{
    let rflags: u64;
    unsafe {
        core::arch::asm!(
            "pushfq",
            "pop {0}",
            out(reg) rflags,
            options(nostack),
        );
    }
    rflags & (1 << 9) != 0
}

/// Send the end-of-interrupt signal to the local APIC.
///
/// Must be called from within an interrupt handler before returning.
/// `_irq` is ignored for xAPIC (EOI register is level-independent).
#[cfg(not(test))]
pub fn acknowledge(_irq: u32)
{
    // SAFETY: direct map is active; APIC EOI write is always safe.
    unsafe {
        apic_write(APIC_EOI, 0);
    }
}

/// No-op test stub.
#[cfg(test)]
pub fn acknowledge(_irq: u32) {}

/// Register a handler for IRQ `irq`.
///
/// Stub only — dynamic IRQ routing is deferred to a later phase.
///
/// # Safety
/// Caller must ensure `handler` is correct for the given IRQ.
pub unsafe fn register_handler(_irq: u32, _handler: fn(u32))
{
    // TODO: implement dynamic handler table in a later phase.
}

/// Mask (disable delivery of) IRQ `irq`.
///
/// Stub — extended in a later phase when the IOAPIC is configured.
pub fn mask(_irq: u32) {}

/// Unmask (enable delivery of) IRQ `irq`.
///
/// Stub — extended in a later phase when the IOAPIC is configured.
pub fn unmask(_irq: u32) {}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn apic_base_phys_constant()
    {
        // Ensure the constant matches the xAPIC fixed address.
        assert_eq!(APIC_BASE_PHYS, 0xFEE0_0000);
    }

    #[test]
    fn apic_svr_offset()
    {
        assert_eq!(APIC_SVR, 0xF0);
    }

    #[test]
    fn apic_eoi_offset()
    {
        assert_eq!(APIC_EOI, 0xB0);
    }

    #[test]
    fn lvt_mask_bit()
    {
        assert_eq!(LVT_MASK, 1 << 16);
    }
}
