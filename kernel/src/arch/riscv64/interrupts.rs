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
/// Handles traps from both U-mode (ecall, page faults) and S-mode (timer,
/// external interrupts). Saves all GPRs and CSRs to a [`TrapFrame`], calls
/// `trap_dispatch`, then restores and executes `sret`.
///
/// ## Stack switching invariant
///
/// `sscratch` encodes the current privilege:
/// - S-mode: `sscratch = 0`
/// - U-mode: `sscratch = kernel stack top for the current thread`
///
/// On U-mode trap entry the handler atomically reads the kernel stack top
/// from `sscratch` (via `csrrw t0, sscratch, t0`) and switches to it before
/// building the [`TrapFrame`]. On exit, `sscratch` is reloaded with the
/// kernel stack top before `sret` returns to U-mode.
///
/// `sscratch` must be initialised to the initial thread's kernel stack top
/// before the first `sret` to U-mode (done in `sched::enter`).
#[cfg(not(test))]
#[unsafe(naked)]
unsafe extern "C" fn trap_entry()
{
    // Frame layout: 34 × 8 = 272 bytes (verified by test below).
    // Offsets: x1=0, x2=8, x3=16, x4=24, x5=32, …, x31=240,
    //          sepc=248, scause=256, stval=264.
    core::arch::naked_asm!(
        // ── Determine source privilege ──────────────────────────────────────────
        // Atomically swap t0 (x5) with sscratch:
        //   t0    = old sscratch (kernel_sp_top if from U-mode, 0 if from S-mode)
        //   sscratch = old t0 (saved here temporarily)
        "csrrw t0, sscratch, t0",
        "bnez t0, 1f",              // t0 != 0 → came from U-mode

        // ── S-mode path ─────────────────────────────────────────────────────────
        // t0 = 0; sscratch = old_t0; sp = kernel_sp (already correct)
        // Restore t0: swap back so t0 = old_t0, sscratch = 0.
        "csrrw t0, sscratch, t0",
        // Allocate TrapFrame on the kernel stack.
        "addi sp, sp, -272",
        // Save t0 (x5) before reusing x5 as a temporary.
        "sd x5, 32(sp)",
        // Record original sp (= current sp + 272, the pre-allocation value).
        "addi x5, sp, 272",
        "sd x5, 8(sp)",
        "j 2f",

        // ── U-mode path ─────────────────────────────────────────────────────────
        // t0 = kernel_sp_top; sscratch = old_t0; sp = user_sp
        "1:",
        // Allocate TrapFrame at the top of the kernel stack (in t0).
        "addi t0, t0, -272",
        // Save user sp (x2) into the frame before overwriting sp.
        "sd x2, 8(t0)",
        // Switch to kernel stack.
        "mv sp, t0",
        // Retrieve original t0 from sscratch; clear sscratch (now in S-mode).
        "csrr x5, sscratch",
        "csrw sscratch, zero",
        "sd x5, 32(sp)",            // frame.t0 = user t0

        // ── Save remaining registers (x2 and x5 already saved above) ───────────
        "2:",
        "sd x1,   0(sp)",           // ra
        // x2 (sp) saved above in both paths
        "sd x3,  16(sp)",           // gp
        "sd x4,  24(sp)",           // tp
        // x5 (t0) saved above in both paths
        "sd x6,  40(sp)",
        "sd x7,  48(sp)",
        "sd x8,  56(sp)",
        "sd x9,  64(sp)",
        "sd x10, 72(sp)",           // a0
        "sd x11, 80(sp)",
        "sd x12, 88(sp)",
        "sd x13, 96(sp)",
        "sd x14,104(sp)",
        "sd x15,112(sp)",
        "sd x16,120(sp)",
        "sd x17,128(sp)",           // a7 (syscall number)
        "sd x18,136(sp)",
        "sd x19,144(sp)",
        "sd x20,152(sp)",
        "sd x21,160(sp)",
        "sd x22,168(sp)",
        "sd x23,176(sp)",
        "sd x24,184(sp)",
        "sd x25,192(sp)",
        "sd x26,200(sp)",
        "sd x27,208(sp)",
        "sd x28,216(sp)",
        "sd x29,224(sp)",
        "sd x30,232(sp)",
        "sd x31,240(sp)",
        // Save supervisor CSRs.
        "csrr t0, sepc",
        "sd   t0, 248(sp)",
        "csrr t0, scause",
        "sd   t0, 256(sp)",
        "csrr t0, stval",
        "sd   t0, 264(sp)",

        // ── Dispatch ────────────────────────────────────────────────────────────
        "mv a0, sp",
        "call {dispatch}",

        // ── Restore sepc ────────────────────────────────────────────────────────
        "ld t0, 248(sp)",
        "csrw sepc, t0",

        // ── Restore sscratch if returning to U-mode ─────────────────────────────
        // Check sstatus.SPP (bit 8): 0 = return to U-mode, 1 = return to S-mode.
        "csrr t0, sstatus",
        "srli t0, t0, 8",
        "andi t0, t0, 1",
        "bnez t0, 3f",
        // Returning to U-mode: sscratch = frame base + 272 = kernel stack top.
        "addi t0, sp, 272",
        "csrw sscratch, t0",

        // ── Restore all registers ────────────────────────────────────────────────
        // x2 (sp) is restored last since it changes the addressing base.
        "3:",
        "ld x1,   0(sp)",
        // x2 restored last
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
        "ld x14,104(sp)",
        "ld x15,112(sp)",
        "ld x16,120(sp)",
        "ld x17,128(sp)",
        "ld x18,136(sp)",
        "ld x19,144(sp)",
        "ld x20,152(sp)",
        "ld x21,160(sp)",
        "ld x22,168(sp)",
        "ld x23,176(sp)",
        "ld x24,184(sp)",
        "ld x25,192(sp)",
        "ld x26,200(sp)",
        "ld x27,208(sp)",
        "ld x28,216(sp)",
        "ld x29,224(sp)",
        "ld x30,232(sp)",
        "ld x31,240(sp)",
        "ld x2,   8(sp)",           // restore sp last (user sp or original kernel sp)
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
                // Supervisor external interrupt: claim, then dispatch.
                // dispatch_external -> dispatch_device_irq calls acknowledge(irq),
                // which writes the PLIC claim/complete register. Do NOT write it
                // again here.
                let irq = plic_read(PLIC_CLAIM_COMPLETE);
                if irq != 0
                {
                    dispatch_external(irq);
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
                let sepc_before = frame.sepc;
                unsafe { crate::syscall::dispatch(frame as *mut _); }
                // Advance sepc past the ecall instruction ONLY if dispatch did
                // not modify sepc. SYS_THREAD_WRITE_REGS may redirect a blocked
                // thread to a new instruction pointer; in that case sepc is
                // already the target address and must not be incremented.
                if frame.sepc == sepc_before
                {
                    frame.sepc += 4;
                }
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

/// Enable PLIC source `source` for hart 0 S-mode context.
///
/// Each source has one bit in a 32-bit enable word. Context 1 (hart 0 S-mode)
/// enable registers start at `PLIC_ENABLE_BASE` in 4-byte words.
///
/// # Safety
/// Direct map must be active; PLIC MMIO must be accessible.
#[cfg(not(test))]
pub fn plic_enable(source: u32)
{
    if source == 0 || source > PLIC_NUM_SOURCES
    {
        return;
    }
    let word_idx = source / 32;
    let bit_idx = source % 32;
    let offset = PLIC_ENABLE_BASE + (word_idx as u64 * 4);
    let current = plic_read(offset);
    // SAFETY: direct map active; PLIC MMIO is accessible.
    unsafe { plic_write(offset, current | (1 << bit_idx)) };
}

/// Disable PLIC source `source` for hart 0 S-mode context.
///
/// # Safety
/// Direct map must be active; PLIC MMIO must be accessible.
#[cfg(not(test))]
pub fn plic_disable(source: u32)
{
    if source == 0 || source > PLIC_NUM_SOURCES
    {
        return;
    }
    let word_idx = source / 32;
    let bit_idx = source % 32;
    let offset = PLIC_ENABLE_BASE + (word_idx as u64 * 4);
    let current = plic_read(offset);
    // SAFETY: direct map active; PLIC MMIO is accessible.
    unsafe { plic_write(offset, current & !(1 << bit_idx)) };
}

/// Mask (disable) PLIC source `irq`.
pub fn mask(irq: u32)
{
    #[cfg(not(test))]
    plic_disable(irq);
    #[cfg(test)]
    let _ = irq;
}

/// Unmask (enable) PLIC source `irq`.
pub fn unmask(irq: u32)
{
    #[cfg(not(test))]
    plic_enable(irq);
    #[cfg(test)]
    let _ = irq;
}

/// Dispatch an external interrupt from the PLIC to its registered signal.
///
/// Called from `trap_dispatch` after claiming the interrupt. Routing is
/// handled by [`crate::irq::dispatch_device_irq`], which masks the source
/// and sends EOI via [`acknowledge`].
///
/// Note: the PLIC complete write (EOI) is performed inside
/// `dispatch_device_irq` via `acknowledge(irq)`, so the caller (`trap_dispatch`)
/// must NOT also write the complete register.
#[cfg(not(test))]
fn dispatch_external(irq: u32)
{
    // SAFETY: called from interrupt context with interrupts disabled.
    unsafe { crate::irq::dispatch_device_irq(irq) };
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

    // Clear sscratch so trap_entry correctly identifies S-mode traps.
    // The UEFI firmware uses sscratch for its own trap handling and may leave
    // it non-zero after ExitBootServices (especially if keyboard interrupts
    // occurred during the firmware phase). A stale non-zero sscratch causes
    // trap_entry to take the U-mode path for an S-mode trap, writing the
    // TrapFrame to a bogus address and faulting.
    unsafe {
        core::arch::asm!("csrw sscratch, zero", options(nostack, nomem));
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

    // Allow U-mode to read the hardware cycle performance counter
    // (scounteren.CY = bit 0). Required for userspace cycle-count benchmarks
    // (equivalent to rdtsc on x86-64). OpenSBI sets mcounteren.CY on QEMU
    // virt so S-mode access is already granted; this propagates it to U-mode.
    // Also enables the VDSO-style clock_gettime fast path in the future libc.
    unsafe {
        core::arch::asm!(
            "csrs scounteren, {cy}",
            cy = in(reg) 1u64,
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
#[allow(dead_code)] // Required by arch interface: kernel/docs/arch-interface.md
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
#[allow(dead_code)] // Required by arch interface: kernel/docs/arch-interface.md
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
