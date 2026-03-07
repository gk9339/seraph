// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/arch/x86_64/syscall.rs

//! SYSCALL/SYSRET MSR setup for x86-64.
//!
//! Installs the SYSCALL entry mechanism by configuring three MSRs:
//!
//! - `IA32_EFER.SCE` — enables the SYSCALL instruction.
//! - `IA32_STAR` — encodes segment selectors:
//!     - bits [47:32]: SYSCALL CS base = 0x08 (kernel CS = 0x08, SS = 0x10).
//!     - bits [63:48]: SYSRET CS base  = 0x10 (user SS = 0x18, user CS = 0x20).
//! - `IA32_LSTAR` — 64-bit entry point address (`syscall_entry`).
//! - `IA32_SFMASK` — clears RFLAGS.IF (bit 9) on SYSCALL entry, preventing
//!   interrupts from arriving with no valid kernel stack pointer set.
//!
//! The `syscall_entry` stub halts immediately: no userspace exists until
//! Phase 9. It is installed so LSTAR points to valid kernel code from boot.
//!
//! # Modification notes
//! - Phase 9: replace `syscall_entry` body with the real register-save,
//!   syscall dispatch, register-restore, and `sysretq` sequence.
//! - If supporting 32-bit compat mode: configure `IA32_CSTAR` for SYSCALL
//!   from ring-3 compatibility mode.

#[cfg(not(test))]
use super::cpu;

// ── MSR addresses ─────────────────────────────────────────────────────────────

/// Extended Feature Enable Register (enables SCE, NXE, etc.).
const IA32_EFER: u32 = 0xC000_0080;
/// Syscall Target Address / Segment selectors.
const IA32_STAR: u32 = 0xC000_0081;
/// Long-mode SYSCALL entry RIP.
const IA32_LSTAR: u32 = 0xC000_0082;
/// SYSCALL flag mask: bits set here are cleared in RFLAGS on SYSCALL entry.
const IA32_SFMASK: u32 = 0xC000_0084;

/// EFER bit 0: System Call Extensions (enables SYSCALL/SYSRET).
const EFER_SCE: u64 = 1 << 0;

/// SFMASK value: clear IF (bit 9) on SYSCALL entry.
const SFMASK_CLEAR_IF: u64 = 1 << 9;

/// STAR value:
/// - bits [47:32] = 0x0008: SYSCALL → CS=0x08 (kernel), SS=0x10.
/// - bits [63:48] = 0x0010: SYSRET  → SS=0x18 (user DS), CS=0x20 (user CS).
///
/// The SYSRET base 0x10 gives: SS = 0x10+8 = 0x18, CS = 0x10+16 = 0x20.
const STAR_VALUE: u64 = (0x0010u64 << 48) | (0x0008u64 << 32);

// ── Phase 5 stub entry ────────────────────────────────────────────────────────

/// SYSCALL entry stub for Phase 5: immediately halts with `ud2`.
///
/// No userspace exists yet; any SYSCALL would be a kernel bug.
/// The stub is installed in LSTAR so the MSR points to valid code.
///
/// Replace this body in Phase 9 with the real syscall dispatch path.
#[cfg(not(test))]
#[unsafe(naked)]
unsafe extern "C" fn syscall_entry()
{
    // No valid user stack here — ud2 is a hard stop.
    // Phase 9: replace with register-save, dispatch, and sysretq.
    core::arch::naked_asm!("ud2");
}

// ── Public interface ──────────────────────────────────────────────────────────

/// Configure the SYSCALL/SYSRET mechanism.
///
/// Sets `IA32_EFER.SCE`, programs `STAR`, `LSTAR`, and `SFMASK`.
/// After this call, user-mode SYSCALL instructions will vector to
/// `syscall_entry` (currently a `ud2` stub).
///
/// # Safety
/// Must execute at ring 0. GDT must be loaded with the correct segment layout
/// (kernel CS=0x08, DS=0x10, user DS=0x18, user CS=0x20) before calling.
#[cfg(not(test))]
pub unsafe fn init()
{
    // SAFETY: ring 0; MSR writes.
    unsafe {
        // Enable SYSCALL instruction in EFER.
        let efer = cpu::read_msr(IA32_EFER);
        cpu::write_msr(IA32_EFER, efer | EFER_SCE);

        // Program segment selectors.
        cpu::write_msr(IA32_STAR, STAR_VALUE);

        // Install entry point.
        cpu::write_msr(IA32_LSTAR, syscall_entry as *const () as u64);

        // Clear IF on SYSCALL entry.
        cpu::write_msr(IA32_SFMASK, SFMASK_CLEAR_IF);
    }
}

/// No-op test stub: MSR writes cannot execute in host unit tests.
#[cfg(test)]
pub unsafe fn init() {}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn star_syscall_cs_is_0x08()
    {
        // STAR bits [47:32] = SYSCALL kernel CS base.
        assert_eq!((STAR_VALUE >> 32) & 0xFFFF, 0x0008);
    }

    #[test]
    fn star_sysret_base_is_0x10()
    {
        // STAR bits [63:48] = SYSRET CS base → SS=0x18, CS=0x20.
        assert_eq!((STAR_VALUE >> 48) & 0xFFFF, 0x0010);
    }

    #[test]
    fn sfmask_clears_if_only()
    {
        // Bit 9 of RFLAGS is IF.
        assert_eq!(SFMASK_CLEAR_IF, 1 << 9);
    }

    #[test]
    fn efer_sce_is_bit_0()
    {
        assert_eq!(EFER_SCE, 1);
    }
}
