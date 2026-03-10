// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/arch/riscv64/interrupts.rs

//! RISC-V trap handling and PLIC initialisation.
//!
//! Sets up the supervisor-mode trap infrastructure:
//! 1. Installs `trap_entry` in `stvec` (direct mode).
//! 2. Clears `sstatus.SIE`, `sstatus.SPP`, `sstatus.SUM` for a clean initial state.
//! 3. Enables `sie.SEIP` (external) and `sie.STIP` (timer) bits.
//! 4. Initialises the PLIC: sets all source priorities to 1 and the hart 0
//!    S-mode threshold to 0 (accept all above-threshold interrupts).
//!
//! The trap vector dispatches:
//! - Timer interrupt (scause = 5 | MSB) → `timer::handle_tick()`
//! - External interrupt (scause = 9 | MSB) → PLIC claim → dispatch → PLIC complete
//! - U-mode ecall (scause = 8) → `syscall::syscall_stub()`
//! - All other exceptions → print diagnostics + `fatal()`
//!
//! # PLIC layout (QEMU virt machine)
//! Base physical address `0x0C00_0000`, accessed via the direct map.
//! - Priority registers: base + 4*source (sources 1–127).
//! - Enable registers:  base + 0x2080 + 4*word  (hart 0 S-mode context).
//! - Threshold:         base + 0x20_1000.
//! - Claim/Complete:    base + 0x20_1004.
//!
//! # Modification notes
//! - To add a new device IRQ: enable its PLIC source in the enable register
//!   and add a case in `dispatch_external`.
//! - To support additional harts: pass the hart ID and update the PLIC
//!   context register offsets (context = hart*2 + 1 for S-mode).

use super::trap_frame::TrapFrame;
use crate::mm::paging::DIRECT_MAP_BASE;

// ── PLIC constants ────────────────────────────────────────────────────────────

/// PLIC physical base address (QEMU virt machine).
const PLIC_BASE_PHYS: u64 = 0x0C00_0000;

/// PLIC priority register base: base + 4 * source_id (source 1..=127).
const PLIC_PRIORITY_BASE: u64 = 0x0000;
/// PLIC enable register base for hart 0 S-mode context (context 1):
///   base + 0x2000 + context*0x80 + word*4.
/// context=1 → offset = 0x2080.
const PLIC_ENABLE_BASE: u64 = 0x2080;
/// PLIC threshold for hart 0 S-mode context (context 1): base + 0x20_0000 + context*0x1000.
/// context=1 → offset = 0x20_1000.
const PLIC_THRESHOLD: u64 = 0x0020_1000;
/// PLIC claim/complete register for hart 0 S-mode context.
const PLIC_CLAIM_COMPLETE: u64 = 0x0020_1004;

/// Number of PLIC interrupt sources supported on the QEMU virt machine.
const PLIC_NUM_SOURCES: u32 = 127;

// ── PLIC access helpers ───────────────────────────────────────────────────────

fn plic_read(offset: u64) -> u32
{
    let vaddr = DIRECT_MAP_BASE + PLIC_BASE_PHYS + offset;
    unsafe { core::ptr::read_volatile(vaddr as *const u32) }
}

unsafe fn plic_write(offset: u64, val: u32)
{
    let vaddr = DIRECT_MAP_BASE + PLIC_BASE_PHYS + offset;
    unsafe { core::ptr::write_volatile(vaddr as *mut u32, val) };
}

// ── Trap vector ───────────────────────────────────────────────────────────────

/// Naked trap entry point installed in `stvec`.
///
/// Saves all general-purpose registers and supervisor CSRs to a `TrapFrame`
/// on the stack, calls `trap_dispatch`, then restores and executes `sret`.
///
/// `sp` is used as the frame pointer; caller must ensure the stack has at
/// least `size_of::<TrapFrame>()` bytes available when a trap fires.
#[cfg(not(test))]
#[unsafe(naked)]
unsafe extern "C" fn trap_entry()
{
    // Frame size: 34 u64 values = 272 bytes.
    core::arch::naked_asm!(
        // Allocate TrapFrame on the stack.
        "addi sp, sp, -272",
        // Save x1–x31 (skip x0 which is always 0).
        "sd x1,   0(sp)",
        "sd x2,   8(sp)",
        "sd x3,  16(sp)",
        "sd x4,  24(sp)",
        "sd x5,  32(sp)",
        "sd x6,  40(sp)",
        "sd x7,  48(sp)",
        "sd x8,  56(sp)",
        "sd x9,  64(sp)",
        "sd x10, 72(sp)",
        "sd x11, 80(sp)",
        "sd x12, 88(sp)",
        "sd x13, 96(sp)",
        "sd x14, 104(sp)",
        "sd x15, 112(sp)",
        "sd x16, 120(sp)",
        "sd x17, 128(sp)",
        "sd x18, 136(sp)",
        "sd x19, 144(sp)",
        "sd x20, 152(sp)",
        "sd x21, 160(sp)",
        "sd x22, 168(sp)",
        "sd x23, 176(sp)",
        "sd x24, 184(sp)",
        "sd x25, 192(sp)",
        "sd x26, 200(sp)",
        "sd x27, 208(sp)",
        "sd x28, 216(sp)",
        "sd x29, 224(sp)",
        "sd x30, 232(sp)",
        "sd x31, 240(sp)",
        // Save supervisor CSRs.
        "csrr t0, sepc",
        "sd   t0, 248(sp)",
        "csrr t0, scause",
        "sd   t0, 256(sp)",
        "csrr t0, stval",
        "sd   t0, 264(sp)",
        // Call trap_dispatch(&mut TrapFrame).
        "mv a0, sp",
        "call {dispatch}",
        // Restore sepc (may have been modified by syscall handler).
        "ld   t0, 248(sp)",
        "csrw sepc, t0",
        // Restore x1–x31.
        "ld x1,   0(sp)",
        "ld x2,   8(sp)",
        "ld x3,  16(sp)",
        "ld x4,  24(sp)",
        "ld x5,  32(sp)",
        "ld x6,  40(sp)",
        "ld x7,  48(sp)",
        "ld x8,  56(sp)",
        "ld x9,  64(sp)",
        "ld x10, 72(sp)",
        "ld x11, 80(sp)",
        "ld x12, 88(sp)",
        "ld x13, 96(sp)",
        "ld x14, 104(sp)",
        "ld x15, 112(sp)",
        "ld x16, 120(sp)",
        "ld x17, 128(sp)",
        "ld x18, 136(sp)",
        "ld x19, 144(sp)",
        "ld x20, 152(sp)",
        "ld x21, 160(sp)",
        "ld x22, 168(sp)",
        "ld x23, 176(sp)",
        "ld x24, 184(sp)",
        "ld x25, 192(sp)",
        "ld x26, 200(sp)",
        "ld x27, 208(sp)",
        "ld x28, 216(sp)",
        "ld x29, 224(sp)",
        "ld x30, 232(sp)",
        "ld x31, 240(sp)",
        // Deallocate frame.
        "addi sp, sp, 272",
        "sret",
        dispatch = sym trap_dispatch,
    );
}

/// Dispatch a trap to the appropriate handler.
///
/// `scause` bit 63 set = interrupt; clear = exception.
/// Called with interrupts disabled (sstatus.SIE is cleared on trap entry).
#[cfg(not(test))]
extern "C" fn trap_dispatch(frame: &mut TrapFrame)
{
    let scause = frame.scause;
    let is_interrupt = scause >> 63 != 0;
    let cause_code = scause & !(1u64 << 63);

    if is_interrupt
    {
        match cause_code
        {
            5 => super::timer::handle_tick(), // Supervisor timer interrupt
            9 =>
            {
                // Supervisor external interrupt: claim, dispatch, complete.
                let irq = plic_read(PLIC_CLAIM_COMPLETE);
                if irq != 0
                {
                    dispatch_external(irq);
                    // SAFETY: direct map active; PLIC MMIO accessible.
                    unsafe {
                        plic_write(PLIC_CLAIM_COMPLETE, irq);
                    }
                }
            }
            _ =>
            {
                crate::kprintln!(
                    "unknown interrupt: scause={:#x} sepc={:#x}",
                    scause,
                    frame.sepc
                );
                crate::fatal("unhandled interrupt");
            }
        }
    }
    else
    {
        match cause_code
        {
            8 =>
            {
                // U-mode ecall: dispatch via the kernel syscall table.
                // SAFETY: frame is a valid TrapFrame on the kernel stack.
                unsafe { crate::syscall::dispatch(frame as *mut _); }
                // Advance sepc past the ecall instruction (4 bytes on RV64).
                frame.sepc += 4;
            }
            _ =>
            {
                crate::kprintln!(
                    "EXCEPTION: scause={:#x} sepc={:#x} stval={:#x}",
                    scause,
                    frame.sepc,
                    frame.stval
                );
                crate::fatal("unhandled exception");
            }
        }
    }
}

/// Dispatch an external interrupt from the PLIC.
///
/// Phase 5 has no device drivers, so all external IRQs are unexpected.
/// Extend this function in later phases to route IRQs to their handlers.
#[cfg(not(test))]
fn dispatch_external(irq: u32)
{
    crate::kprintln!("unexpected external IRQ: {}", irq);
}

// ── Public interface ──────────────────────────────────────────────────────────

/// Initialise trap handling and the PLIC.
///
/// Must be called once during Phase 5 from a single-threaded context.
///
/// # Safety
/// Must execute in supervisor mode with the direct physical map active.
#[cfg(not(test))]
pub unsafe fn init()
{
    // Install trap vector (direct mode: bit [1:0] = 00).
    // SAFETY: trap_entry is a valid naked function at a known address.
    unsafe {
        core::arch::asm!(
            "csrw stvec, {0}",
            in(reg) trap_entry as *const () as u64,
            options(nostack, nomem),
        );
    }

    // Clear sstatus.SIE (bit 1), sstatus.SPP (bit 8), sstatus.SUM (bit 18).
    // SIE: global interrupt enable — starts disabled, timer::init() enables it.
    // SPP: previous privilege (0 = U-mode return target).
    // SUM: permit S-mode to access U-mode pages (not needed; keep disabled).
    unsafe {
        core::arch::asm!(
            "csrc sstatus, {mask}",
            mask = in(reg) (1u64 << 1) | (1u64 << 8) | (1u64 << 18),
            options(nostack, nomem),
        );
    }

    // Enable SEIP (bit 9) and STIP (bit 5) in sie.
    unsafe {
        core::arch::asm!(
            "csrs sie, {mask}",
            mask = in(reg) (1u64 << 9) | (1u64 << 5),
            options(nostack, nomem),
        );
    }

    // Initialise PLIC:
    // - Set priority 1 for all sources (0 = disabled, 1 = lowest priority).
    // - Set threshold to 0 for hart 0 S-mode context (accept all sources ≥ 1).
    // SAFETY: direct map active; PLIC MMIO region is accessible.
    unsafe {
        for src in 1..=PLIC_NUM_SOURCES
        {
            plic_write(PLIC_PRIORITY_BASE + (src as u64 * 4), 1);
        }
        plic_write(PLIC_THRESHOLD, 0);
    }
}

/// Disable supervisor interrupts. Returns previous SIE state.
pub fn disable() -> bool
{
    let prev: u64;
    unsafe {
        core::arch::asm!(
            "csrrci {0}, sstatus, 0x2",
            out(reg) prev,
            options(nostack, nomem),
        );
    }
    prev & (1 << 1) != 0 // SIE is bit 1 of sstatus
}

/// Enable supervisor interrupts.
///
/// # Safety
/// Trap vector must be installed before calling.
pub unsafe fn enable()
{
    unsafe {
        core::arch::asm!("csrsi sstatus, 0x2", options(nostack, nomem));
    }
}

/// Return `true` if supervisor interrupts are currently enabled.
pub fn are_enabled() -> bool
{
    let sstatus: u64;
    unsafe {
        core::arch::asm!(
            "csrr {0}, sstatus",
            out(reg) sstatus,
            options(nostack, nomem),
        );
    }
    sstatus & (1 << 1) != 0
}

/// Complete a PLIC external interrupt for `irq`.
///
/// Must be called after servicing the interrupt; called internally by the
/// trap dispatcher after `dispatch_external`.
pub fn acknowledge(irq: u32)
{
    unsafe {
        plic_write(PLIC_CLAIM_COMPLETE, irq);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn plic_base_phys()
    {
        assert_eq!(PLIC_BASE_PHYS, 0x0C00_0000);
    }

    #[test]
    fn plic_threshold_offset()
    {
        assert_eq!(PLIC_THRESHOLD, 0x0020_1000);
    }

    #[test]
    fn plic_claim_complete_offset()
    {
        assert_eq!(PLIC_CLAIM_COMPLETE, 0x0020_1004);
    }

    #[test]
    fn trap_frame_size()
    {
        // 31 regs × 8 + sepc + scause + stval = 34 × 8 = 272 bytes.
        assert_eq!(core::mem::size_of::<TrapFrame>(), 272);
    }
}
