// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/arch/riscv64/cpu.rs

//! RISC-V 64-bit CPU control primitives.
//!
//! # Phase 5 additions
//! - `halt_until_interrupt` — execute `wfi` with SIE enabled so the timer fires.
//! - `current_id` — returns 0 (BSP only; real hart ID deferred to SMP phase).

// ── Phase 5 additions ─────────────────────────────────────────────────────────

/// Suspend the hart until the next interrupt fires, then return.
///
/// On RISC-V, `wfi` only wakes for **enabled** interrupts (SIE must be
/// set). The caller (idle loop) keeps SIE=1 on RISC-V, so `wfi` wakes
/// on the next timer tick, wakeup IPI, or external interrupt.
///
/// A wakeup IPI can be consumed between the idle loop's `has_runnable`
/// check and `wfi`. The IPI handler sets a reschedule-pending flag that
/// the idle loop checks before calling this function; this catches most
/// cases. For the remaining window (IPI between flag check and `wfi`),
/// the timer interrupt (10 ms) provides a bounded fallback wakeup.
pub fn halt_until_interrupt()
{
    // SAFETY: wfi suspends until an enabled interrupt arrives.
    // SIE is already set by the caller (idle loop keeps SIE=1 on RISC-V).
    unsafe {
        core::arch::asm!("wfi", options(nostack, nomem));
    }
}

/// Return the current hart ID.
///
/// Phase 5: only the BSP is running; returns 0.
/// Future: read `mhartid` via SBI `sbi_get_marchid` or from the boot-info
/// structure when SMP is brought up.
#[allow(dead_code)] // Required by arch interface: kernel/docs/arch-interface.md
pub fn current_id() -> u32
{
    0
}

// ── Per-CPU tp register ───────────────────────────────────────────────────────

/// Install `addr` as the per-CPU data pointer for the current hart.
///
/// Writes `addr` into the `tp` (thread pointer) register so that
/// `current_cpu()` can recover the hart's `PerCpuData` without a
/// global lookup. Must be called from Phase 5 (BSP) and
/// `kernel_entry_ap` (each AP) before any `current_cpu()` call.
///
/// # Safety
/// Must execute in supervisor mode. `addr` must be the virtual address
/// of a valid `PerCpuData` that outlives the hart's execution.
#[cfg(not(test))]
pub unsafe fn install_percpu(addr: u64)
{
    // SAFETY: writing tp is always safe in S-mode; addr is valid per caller.
    unsafe {
        core::arch::asm!(
            "mv tp, {}",
            in(reg) addr,
            options(nostack, nomem),
        );
    }
}

/// Return the logical CPU index of the executing hart.
///
/// Reads `PerCpuData::cpu_id` (u32 at offset 0) via the `tp` register
/// which was set by [`install_percpu`].
///
/// On non-test builds this dereferences `tp`; in test builds returns 0.
pub fn current_cpu() -> u32
{
    #[cfg(not(test))]
    {
        let tp: u64;
        // SAFETY: reading tp is always safe; install_percpu ensures it points
        // to a valid PerCpuData with cpu_id at offset 0.
        unsafe {
            core::arch::asm!(
                "mv {}, tp",
                out(reg) tp,
                options(nostack, nomem),
            );
            *(tp as *const u32)
        }
    }
    #[cfg(test)]
    {
        0
    }
}

// ── Kernel trap stack ─────────────────────────────────────────────────────────

/// Set the kernel stack pointer used when a trap fires from U-mode.
///
/// On RISC-V, `sscratch` holds `&PER_CPU[cpu_id]` (not the stack pointer)
/// so `trap_entry` can recover the per-CPU pointer on U-mode entry.  The
/// actual kernel stack top is stored in `PerCpuData::kernel_rsp` (offset 8
/// from `tp`), from which `trap_entry` loads it when switching stacks.
///
/// Must be called before the first `sret` to U-mode and again whenever the
/// current thread changes.
///
/// # Safety
/// Must execute in supervisor mode.  `tp` must already point to a valid
/// `PerCpuData` (guaranteed after [`install_percpu`] is called in Phase 5).
#[cfg(not(test))]
#[inline]
pub unsafe fn set_kernel_trap_stack(stack_top: u64)
{
    // SAFETY: tp = &PER_CPU[cpu_id] (installed in Phase 5). offset 8 =
    // PerCpuData::kernel_rsp. Storing via tp is safe in S-mode.
    unsafe {
        core::arch::asm!(
            "sd {}, 8(tp)",
            in(reg) stack_top,
            options(nostack),
        );
    }
}

// ── SUM user-access bracket ───────────────────────────────────────────────────

/// Allow supervisor-mode access to user pages (sets sstatus.SUM, bit 18).
///
/// Must be paired with a matching `user_access_end` call.
///
/// # Safety
/// Must execute in supervisor mode. Leaves SUM set until `user_access_end`.
///
/// # Compiler barrier
/// `nomem` is intentionally absent so the compiler treats this CSR write as a
/// memory operation. This prevents the compiler from reordering user-memory
/// loads to before the csrrs at opt-level ≥ 1, matching Linux's "memory"
/// clobber on equivalent operations.
#[cfg(not(test))]
#[inline]
pub unsafe fn user_access_begin()
{
    // SAFETY: csrrs sets bit 18 (SUM) in sstatus; safe in supervisor mode.
    // csrsi/csrci only accept 5-bit immediates (0-31); bit 18 must use a register.
    // nostack: CSR write does not modify sp.
    // (no nomem): compiler memory barrier — prevents hoisting user-memory loads
    // above this instruction at opt-level ≥ 1.
    unsafe {
        core::arch::asm!(
            "csrrs zero, sstatus, {sum}",
            sum = in(reg) (1u64 << 18),
            options(nostack),
        );
    }
}

/// Revoke supervisor-mode access to user pages (clears sstatus.SUM, bit 18).
///
/// # Safety
/// Must be called after a matching `user_access_begin`.
///
/// # Compiler barrier
/// Like `user_access_begin`, `nomem` is absent to prevent the compiler from
/// sinking user-memory stores to after the csrrc.
#[cfg(not(test))]
#[inline]
pub unsafe fn user_access_end()
{
    // SAFETY: csrrc clears bit 18 (SUM) in sstatus; restores user-page isolation.
    unsafe {
        core::arch::asm!(
            "csrrc zero, sstatus, {sum}",
            sum = in(reg) (1u64 << 18),
            options(nostack),
        );
    }
}

// ── Interrupt save/restore ────────────────────────────────────────────────────

/// Save the current interrupt-enable state and disable supervisor interrupts.
/// Returns the sstatus value at the time of the call (opaque to callers).
///
/// # Safety
/// Must execute in supervisor mode.
#[cfg(not(test))]
#[inline]
pub unsafe fn save_and_disable_interrupts() -> u64
{
    let sstatus: u64;
    // SAFETY: csrrci atomically reads sstatus and clears the SIE bit.
    unsafe {
        core::arch::asm!(
            "csrrci {sstatus}, sstatus, 2",
            sstatus = out(reg) sstatus,
            options(nostack, nomem),
        );
    }
    sstatus
}

/// Restore the interrupt-enable state saved by [`save_and_disable_interrupts`].
///
/// # Safety
/// Must execute in supervisor mode. `saved` must be a value returned by
/// `save_and_disable_interrupts` on this hart.
#[cfg(not(test))]
#[inline]
pub unsafe fn restore_interrupts(saved: u64)
{
    let sie_bit = (saved >> 1) & 1;
    if sie_bit != 0
    {
        // SAFETY: re-enabling SIE after we previously cleared it.
        unsafe {
            core::arch::asm!("csrsi sstatus, 2", options(nostack, nomem));
        }
    }
}

// ── Interrupt control ─────────────────────────────────────────────────────────

/// Disable supervisor-mode interrupts via sstatus.SIE.
///
/// # Safety
/// Changes global CPU interrupt state. Caller is responsible for managing
/// interrupt state across the transition.
pub unsafe fn disable_interrupts()
{
    // SAFETY: csrci clears the SIE bit (bit 1) in sstatus.
    // Caller guarantees this is called in supervisor mode.
    unsafe {
        core::arch::asm!("csrci sstatus, 0x2", options(nomem, nostack));
    }
}

/// Disable interrupts and halt the CPU permanently using `wfi`.
///
/// `wfi` (wait-for-interrupt) suspends the hart until an interrupt arrives.
/// With SIE cleared the hart cannot actually handle the interrupt, so it
/// re-executes `wfi` immediately — achieving an effective halt without a
/// busy spin.
pub fn halt_loop() -> !
{
    // SAFETY: disabling interrupts before wfi is required for a safe permanent halt.
    unsafe {
        disable_interrupts();
    }
    loop
    {
        // SAFETY: wfi is a hint that the hart may be suspended; safe at any privilege level.
        unsafe {
            core::arch::asm!("wfi", options(nomem, nostack));
        }
    }
}
