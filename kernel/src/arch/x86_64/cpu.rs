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

// ── SMAP user-access bracket ──────────────────────────────────────────────────

/// Allow supervisor-mode access to user pages (sets AC flag via `stac`).
///
/// Must be paired with a matching `user_access_end` call. Nesting is not
/// supported. Call immediately before reading/writing user memory, and call
/// `user_access_end` immediately after.
///
/// # Safety
/// Must execute at ring 0. Leaves AC set until `user_access_end` is called,
/// so any faulting user-pointer dereference between the two calls will not
/// produce a SMAP fault (but may still fault for other reasons).
///
/// # Compiler barrier
/// `nomem` is intentionally absent so the compiler treats this as a memory
/// operation. This prevents the compiler from reordering user-memory accesses
/// (loads OR stores) to before the stac at opt-level ≥ 1. Mirrors Linux's
/// `stac()` which uses an asm "memory" clobber for the same reason.
#[cfg(not(test))]
#[inline]
pub unsafe fn user_access_begin()
{
    // SAFETY: stac sets AC in RFLAGS; safe at ring 0 when SMAP is enabled.
    // nostack: stac does not modify RSP.
    // (no nomem): acts as a compiler memory barrier — prevents the optimizer
    // from hoisting user-memory accesses above this instruction.
    unsafe {
        core::arch::asm!("stac", options(nostack));
    }
}

/// Revoke supervisor-mode access to user pages (clears AC flag via `clac`).
///
/// # Safety
/// Must be called after a matching `user_access_begin`.
///
/// # Compiler barrier
/// Like `user_access_begin`, `nomem` is absent to prevent the compiler from
/// sinking user-memory accesses to after the clac.
#[cfg(not(test))]
#[inline]
pub unsafe fn user_access_end()
{
    // SAFETY: clac clears AC in RFLAGS; restores SMAP protection.
    unsafe {
        core::arch::asm!("clac", options(nostack));
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
#[allow(dead_code)] // Required by arch interface: kernel/docs/arch-interface.md
pub fn current_id() -> u32
{
    let (_eax, ebx, _ecx, _edx) = cpuid(1);
    ebx >> 24
}

// ── Per-CPU GS-base ───────────────────────────────────────────────────────────

/// MSR address for `IA32_GS_BASE` — the canonical GS segment base.
const IA32_GS_BASE: u32 = 0xC000_0101;

/// Install `addr` as the per-CPU data pointer for the current CPU.
///
/// Writes `addr` to `IA32_GS_BASE` (MSR 0xC000_0101) so that
/// GS-relative loads (`gs:[offset]`) reach the `PerCpuData` entry for
/// this CPU. Must be called from Phase 5 (BSP) and `kernel_entry_ap`
/// (each AP) before any GS-relative access occurs.
///
/// # Safety
/// Must execute at ring 0. `addr` must be the virtual address of a valid
/// `PerCpuData` that outlives the CPU's execution.
#[cfg(not(test))]
pub unsafe fn install_percpu(addr: u64)
{
    // SAFETY: IA32_GS_BASE is a valid MSR on all x86-64 CPUs; ring 0.
    unsafe {
        write_msr(IA32_GS_BASE, addr);
    }
}

/// Return the logical CPU index of the executing CPU.
///
/// Reads `gs:[0]` which holds `PerCpuData::cpu_id` (u32, offset 0).
/// Valid after [`install_percpu`] is called for this CPU.
///
/// # Safety (internal)
/// `gs:[0]` is always a valid u32 read once GS-base is installed.
/// The function is safe to call because the install guarantee is a
/// precondition of the kernel running on this CPU.
pub fn current_cpu() -> u32
{
    #[cfg(not(test))]
    {
        let id: u32;
        // SAFETY: gs:[0] == PerCpuData::cpu_id; valid after install_percpu.
        unsafe {
            core::arch::asm!(
                "mov {:e}, gs:[0]",
                out(reg) id,
                options(nostack, readonly, preserves_flags),
            );
        }
        id
    }
    #[cfg(test)]
    {
        0
    }
}

// ── Kernel trap stack ─────────────────────────────────────────────────────────

/// Set the kernel stack pointer used when a trap fires from U-mode.
///
/// On x86-64 this requires two writes: TSS RSP0 (for hardware interrupt/
/// exception entry) and `SYSCALL_KERNEL_RSP` (for the `SYSCALL` fast path).
/// Must be called on every context switch to a user thread.
///
/// # Safety
/// Must execute at ring 0. Caller must ensure the stack is valid.
#[cfg(not(test))]
#[inline]
pub unsafe fn set_kernel_trap_stack(stack_top: u64)
{
    unsafe {
        super::gdt::set_rsp0(stack_top);
        super::syscall::set_kernel_rsp(stack_top);
    }
}

/// Save the current interrupt-enable state and disable hardware interrupts.
/// Returns an opaque value to pass to [`restore_interrupts`].
///
/// # Safety
/// Must execute at ring 0.
#[cfg(not(test))]
#[inline]
pub unsafe fn save_and_disable_interrupts() -> u64
{
    let flags: u64;
    // SAFETY: pushfq/popfq are valid at ring 0; cli is safe here.
    unsafe {
        core::arch::asm!(
            "pushfq",
            "pop {flags}",
            "cli",
            flags = out(reg) flags,
            options(nostack),
        );
    }
    flags
}

/// Restore the interrupt-enable state saved by [`save_and_disable_interrupts`].
///
/// # Safety
/// Must execute at ring 0. `saved` must be a value returned by
/// `save_and_disable_interrupts` on this CPU.
#[cfg(not(test))]
#[inline]
pub unsafe fn restore_interrupts(saved: u64)
{
    // SAFETY: restoring a previously captured FLAGS value is safe.
    unsafe {
        core::arch::asm!(
            "push {flags}",
            "popfq",
            flags = in(reg) saved,
            options(nostack),
        );
    }
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
