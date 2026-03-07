// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/arch/x86_64/cpu.rs

//! x86-64 CPU control primitives.
//!
//! # Phase 5 additions
//! - `cpuid` — execute CPUID with a given leaf.
//! - `read_cr4` / `write_cr4` — CR4 access.
//! - `read_msr` / `write_msr` — MSR access.
//! - `enable_smep_smap` — verify CPUID support and set CR4 bits 20+21.
//! - `halt_until_interrupt` — `sti; hlt` (allows timer to fire).
//! - `current_id` — return LAPIC ID from CPUID.01H.
//!
//! All privileged instructions are guarded with `#[cfg(not(test))]` so unit
//! tests can run on the host without requiring kernel privilege.

// ── CPUID ─────────────────────────────────────────────────────────────────────

/// Execute CPUID with leaf `leaf`. Returns `(eax, ebx, ecx, edx)`.
///
/// `rbx` is callee-saved and also used internally by LLVM, so it is
/// preserved via a push/pop around the `cpuid` instruction.
pub fn cpuid(leaf: u32) -> (u32, u32, u32, u32)
{
    let eax: u32;
    let ebx: u32;
    let ecx: u32;
    let edx: u32;
    // SAFETY: CPUID is always available on x86-64; read-only.
    unsafe {
        core::arch::asm!(
            "push rbx",
            "cpuid",
            "mov {ebx_out:e}, ebx",
            "pop rbx",
            inout("eax") leaf => eax,
            ebx_out = lateout(reg) ebx,
            inout("ecx") 0u32 => ecx,
            lateout("edx") edx,
            // nostack omitted: push/pop modifies the stack pointer.
        );
    }
    (eax, ebx, ecx, edx)
}

// ── CR4 ───────────────────────────────────────────────────────────────────────

/// Read the current value of CR4.
#[cfg(not(test))]
pub fn read_cr4() -> u64
{
    let val: u64;
    // SAFETY: CR4 readable at ring 0.
    unsafe {
        core::arch::asm!("mov {}, cr4", out(reg) val, options(nostack, nomem));
    }
    val
}

/// Write `val` to CR4.
///
/// # Safety
/// Caller must ensure the new CR4 value is valid and will not fault.
#[cfg(not(test))]
pub unsafe fn write_cr4(val: u64)
{
    // SAFETY: caller's responsibility.
    unsafe {
        core::arch::asm!("mov cr4, {}", in(reg) val, options(nostack, nomem));
    }
}

// ── MSR ───────────────────────────────────────────────────────────────────────

/// Read a model-specific register `msr`.
///
/// # Safety
/// Must execute at ring 0. The MSR must exist on this CPU.
#[cfg(not(test))]
pub unsafe fn read_msr(msr: u32) -> u64
{
    let lo: u32;
    let hi: u32;
    // SAFETY: caller guarantees ring 0 and valid MSR.
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") lo,
            out("edx") hi,
            options(nostack, nomem),
        );
    }
    ((hi as u64) << 32) | lo as u64
}

/// Write `val` to model-specific register `msr`.
///
/// # Safety
/// Must execute at ring 0. The MSR must exist and the value must be valid.
#[cfg(not(test))]
pub unsafe fn write_msr(msr: u32, val: u64)
{
    let lo = (val & 0xFFFF_FFFF) as u32;
    let hi = (val >> 32) as u32;
    // SAFETY: caller's responsibility.
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") lo,
            in("edx") hi,
            options(nostack, nomem),
        );
    }
}

// ── SMEP / SMAP ───────────────────────────────────────────────────────────────

/// Enable Supervisor Mode Execution Prevention (SMEP) and Supervisor Mode
/// Access Prevention (SMAP) by setting CR4 bits 20 and 21.
///
/// Checks CPUID.07H:EBX bit 7 (SMEP) and bit 20 (SMAP). Halts with a fatal
/// message if either feature is absent, because the security model requires
/// both.
///
/// # Safety
/// Must execute at ring 0. May only be called after the IDT is loaded so that
/// a CR4 write fault is catchable (in practice SMEP/SMAP are always present
/// on any QEMU configuration we support).
#[cfg(not(test))]
pub unsafe fn enable_smep_smap()
{
    // CPUID leaf 7, sub-leaf 0.
    let (_eax, ebx, _ecx, _edx) = cpuid(7);
    let smep_present = (ebx >> 7) & 1 != 0;
    let smap_present = (ebx >> 20) & 1 != 0;
    if !smep_present
    {
        crate::fatal("SMEP not supported by CPU — required");
    }
    if !smap_present
    {
        crate::fatal("SMAP not supported by CPU — required");
    }
    // Bit 20 = SMEP, bit 21 = SMAP.
    // SAFETY: CPUID confirmed both features.
    let cr4 = read_cr4();
    unsafe {
        write_cr4(cr4 | (1 << 20) | (1 << 21));
    }
}

// ── Misc ──────────────────────────────────────────────────────────────────────

/// Enable interrupts and halt until the next interrupt fires, then return.
///
/// Used in the idle loop once the timer is running. Unlike `halt_loop`, this
/// re-enables interrupts so the preemption timer can fire.
pub fn halt_until_interrupt()
{
    // SAFETY: sti enables interrupts, hlt suspends until one arrives.
    // The instruction sequence is atomic: no interrupt can occur between
    // sti and hlt (x86 guarantee).
    unsafe {
        core::arch::asm!("sti; hlt", options(nostack, nomem));
    }
}

/// Return the local APIC ID of the current CPU (from CPUID.01H:EBX[31:24]).
///
/// Phase 5 only starts the BSP (Bootstrap Processor); this will return 0
/// on a single-CPU QEMU configuration.
pub fn current_id() -> u32
{
    let (_eax, ebx, _ecx, _edx) = cpuid(1);
    ebx >> 24
}

// ── Interrupts (hardware state) ───────────────────────────────────────────────

/// Disable hardware interrupts.
///
/// # Safety
/// Changes global CPU interrupt state. Caller is responsible for re-enabling
/// interrupts when appropriate (the kernel does not enable them during early boot).
pub unsafe fn disable_interrupts()
{
    // SAFETY: caller guarantees this is called in an appropriate context.
    unsafe {
        core::arch::asm!("cli", options(nomem, nostack, preserves_flags));
    }
}

/// Disable interrupts and halt the CPU permanently.
///
/// Loops on `hlt` so that any NMI that fires during early boot does not cause
/// an uncontrolled jump; interrupts remain disabled.
pub fn halt_loop() -> !
{
    // SAFETY: cli disables interrupts; hlt is safe to execute at any privilege level.
    unsafe {
        disable_interrupts();
    }
    loop
    {
        // SAFETY: hlt puts the CPU into a low-power wait state until the next interrupt.
        // Interrupts are disabled above, so this halts permanently.
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}
