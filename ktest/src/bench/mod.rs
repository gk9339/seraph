// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// ktest/src/bench/mod.rs

//! Tier 3 — Benchmarks / profiling.
//!
//! Each benchmark runs an operation N times and logs min/mean/max cycle counts
//! to the kernel serial console. No PASS/FAIL verdict is produced; the numbers
//! are for human inspection and regression tracking.
//!
//! # Cycle counter access
//!
//! Benchmarks read the hardware cycle counter directly — no syscall overhead
//! on the measurement path:
//!
//! - **x86-64**: `rdtsc` (accessible from U-mode by default; CR4.TSD is not
//!   set by the kernel).
//! - **RISC-V**: `csrr cycle` (accessible from U-mode after the kernel sets
//!   `scounteren.CY = 1` during Phase 5 init).
//!
//! # Adding a new benchmark
//!
//! 1. Write a `fn bench_<name>(ctx: &crate::TestContext)` function below.
//! 2. Call it from `run_all`.
//! 3. Use `cycles_now()` to bracket the measured operation.
//! 4. Log results with `crate::log_u64` (no heap required).

use syscall::{
    cap_copy, cap_create_cspace, cap_create_endpoint, cap_create_signal, cap_create_thread,
    cap_delete, ipc_recv, ipc_reply, signal_send, signal_wait, thread_configure, thread_exit,
    thread_start,
};
use syscall_abi::SystemInfoType;

use crate::ChildStack;

// ── Cycle counter ─────────────────────────────────────────────────────────────

/// Read the hardware cycle counter.
///
/// On x86-64, uses `rdtsc`. On RISC-V, uses `csrr cycle`.
/// On RISC-V, requires the kernel to have set `scounteren.CY = 1`.
///
/// Returns raw cycle counts. Units differ by architecture; use deltas only.
#[inline(always)]
fn cycles_now() -> u64
{
    #[cfg(target_arch = "x86_64")]
    {
        let lo: u32;
        let hi: u32;
        // SAFETY: rdtsc is a user-mode instruction when CR4.TSD = 0 (the
        // kernel does not set TSD). preserves_flags: rdtsc does not modify
        // RFLAGS, only EAX/EDX.
        unsafe {
            core::arch::asm!(
                "rdtsc",
                out("eax") lo,
                out("edx") hi,
                options(nostack, nomem, preserves_flags),
            );
        }
        (hi as u64) << 32 | lo as u64
    }
    #[cfg(target_arch = "riscv64")]
    {
        let c: u64;
        // SAFETY: cycle CSR is accessible from U-mode when scounteren.CY = 1,
        // which the kernel sets during Phase 5 init.
        unsafe {
            core::arch::asm!(
                "csrr {}, cycle",
                out(reg) c,
                options(nostack, nomem),
            );
        }
        c
    }
}

// ── Benchmarks ────────────────────────────────────────────────────────────────

/// Benchmark: null syscall round-trip (kernel entry + exit cost baseline).
///
/// Calls `SYS_SYSTEM_INFO(KernelVersion)` N times. This syscall does a single
/// atomic load and returns — the measured cost is dominated by the
/// `SYSCALL`/`SYSRET` (x86-64) or `ECALL`/`SRET` (RISC-V) transition plus
/// the kernel dispatch overhead, not any meaningful kernel-side work.
///
/// Reports min/mean/max cycle deltas to the serial console.
fn bench_null_syscall(_ctx: &crate::TestContext)
{
    const N: u64 = 1000;
    let mut min = u64::MAX;
    let mut max = 0u64;
    let mut total = 0u64;

    for _ in 0..N
    {
        let t0 = cycles_now();
        // SYS_SYSTEM_INFO with a known variant: one atomic load, then SYSRET/SRET.
        let _ = syscall::system_info(SystemInfoType::KernelVersion as u64);
        let t1 = cycles_now();
        let delta = t1.saturating_sub(t0);
        if delta < min
        {
            min = delta;
        }
        if delta > max
        {
            max = delta;
        }
        total = total.saturating_add(delta);
    }

    crate::klog("ktest: bench  null_syscall_roundtrip  N=1000");
    crate::log_u64("ktest: bench  cycles_min=", min);
    crate::log_u64("ktest: bench  cycles_mean=", total / N);
    crate::log_u64("ktest: bench  cycles_max=", max);
}

// ── IPC round-trip benchmark ──────────────────────────────────────────────────

// Static child stack for the IPC responder thread.
static mut BENCH_IPC_STACK: ChildStack = ChildStack::ZERO;

/// Child entry for `bench_ipc_round_trip`.
///
/// Calls the endpoint N times with label 0 and no data. Exits when done.
/// `arg`: bits[15:0] = ep_slot, bits[31:16] = done_slot, bits[47:32] = N.
fn ipc_caller_entry(arg: u64) -> !
{
    let ep_slot = (arg & 0xFFFF) as u32;
    let done_slot = ((arg >> 16) & 0xFFFF) as u32;
    let n = (arg >> 32) as u64;

    for _ in 0..n
    {
        if syscall::ipc_call(ep_slot, 0, 0, &[]).is_err()
        {
            break;
        }
    }
    signal_send(done_slot, 1).ok();
    thread_exit()
}

/// Benchmark: synchronous IPC round-trip (server side).
///
/// Spawns a child thread that calls the endpoint N times. The server loops
/// `ipc_recv` + `ipc_reply` N times, bracketing the total with cycle counters.
/// Reports mean cycles per round-trip to the serial console.
///
/// This measures: kernel entry, IPC state machine (send-queue dequeue, thread
/// wakeup), and return — the dominant real-world kernel operation cost.
fn bench_ipc_round_trip(ctx: &crate::TestContext)
{
    const N: u64 = 1000;

    // SEND | GRANT rights for the child endpoint copy.
    const RIGHTS_SEND_GRANT: u64 = (1 << 4) | (1 << 6);

    let ep = match cap_create_endpoint()
    {
        Ok(v) => v,
        Err(_) => return,
    };
    let done = match cap_create_signal()
    {
        Ok(v) => v,
        Err(_) => return,
    };

    let cs = match cap_create_cspace(16)
    {
        Ok(v) => v,
        Err(_) => return,
    };
    let child_ep = match cap_copy(ep, cs, RIGHTS_SEND_GRANT)
    {
        Ok(v) => v,
        Err(_) => return,
    };
    let child_done = match cap_copy(done, cs, 1 << 7)
    {
        Ok(v) => v,
        Err(_) => return,
    };

    let arg = (child_ep as u64) | ((child_done as u64) << 16) | (N << 32);
    let th = match cap_create_thread(ctx.aspace_cap, cs)
    {
        Ok(v) => v,
        Err(_) => return,
    };
    let stack_top = ChildStack::top(core::ptr::addr_of!(BENCH_IPC_STACK));

    if thread_configure(th, ipc_caller_entry as *const () as u64, stack_top, arg).is_err()
        || thread_start(th).is_err()
    {
        return;
    }

    let t0 = cycles_now();
    for _ in 0..N
    {
        if ipc_recv(ep).is_err()
        {
            break;
        }
        if ipc_reply(0, 0, &[]).is_err()
        {
            break;
        }
    }
    let t1 = cycles_now();

    // Wait for child to finish before cleaning up caps.
    signal_wait(done).ok();

    let total = t1.saturating_sub(t0);
    crate::klog("ktest: bench  ipc_round_trip  N=1000");
    crate::log_u64("ktest: bench  cycles_mean=", total / N);

    cap_delete(th).ok();
    cap_delete(ep).ok();
    cap_delete(done).ok();
    cap_delete(cs).ok();
}

// ── Signal round-trip benchmark ───────────────────────────────────────────────

// Static child stack for the signal ping-pong responder.
static mut BENCH_SIGNAL_STACK: ChildStack = ChildStack::ZERO;

/// Child entry for `bench_signal_roundtrip`.
///
/// Waits on `in_slot` then sends 1 bit on `out_slot`, repeated N times.
/// `arg`: bits[15:0] = in_slot, bits[31:16] = out_slot, bits[47:32] = N.
fn signal_pong_entry(arg: u64) -> !
{
    let in_slot = (arg & 0xFFFF) as u32;
    let out_slot = ((arg >> 16) & 0xFFFF) as u32;
    let n = (arg >> 32) as u64;

    for _ in 0..n
    {
        if signal_wait(in_slot).is_err()
        {
            break;
        }
        if signal_send(out_slot, 1).is_err()
        {
            break;
        }
    }
    thread_exit()
}

/// Benchmark: signal ping-pong round-trip.
///
/// Main thread sends 1 bit on `ping`, child waits on `ping` and sends 1 bit
/// back on `pong`, main waits on `pong`. N=1000 iterations; measures total
/// round-trip cycles / N — captures signal_send + thread wakeup latency.
fn bench_signal_roundtrip(ctx: &crate::TestContext)
{
    const N: u64 = 1000;

    // SIGNAL right (bit 7) and WAIT right (bit 8).
    const RIGHTS_SIGNAL: u64 = 1 << 7;
    const RIGHTS_WAIT: u64 = 1 << 8;

    let ping = match cap_create_signal()
    {
        Ok(v) => v,
        Err(_) => return,
    };
    let pong = match cap_create_signal()
    {
        Ok(v) => v,
        Err(_) => return,
    };

    let cs = match cap_create_cspace(16)
    {
        Ok(v) => v,
        Err(_) => return,
    };
    // Child waits on ping → needs WAIT right; sends on pong → needs SIGNAL right.
    let child_ping = match cap_copy(ping, cs, RIGHTS_WAIT)
    {
        Ok(v) => v,
        Err(_) => return,
    };
    let child_pong = match cap_copy(pong, cs, RIGHTS_SIGNAL)
    {
        Ok(v) => v,
        Err(_) => return,
    };

    let arg = (child_ping as u64) | ((child_pong as u64) << 16) | (N << 32);
    let th = match cap_create_thread(ctx.aspace_cap, cs)
    {
        Ok(v) => v,
        Err(_) => return,
    };
    let stack_top = ChildStack::top(core::ptr::addr_of!(BENCH_SIGNAL_STACK));

    if thread_configure(th, signal_pong_entry as *const () as u64, stack_top, arg).is_err()
        || thread_start(th).is_err()
    {
        return;
    }

    let t0 = cycles_now();
    for _ in 0..N
    {
        if signal_send(ping, 1).is_err()
        {
            break;
        }
        if signal_wait(pong).is_err()
        {
            break;
        }
    }
    let t1 = cycles_now();

    let total = t1.saturating_sub(t0);
    crate::klog("ktest: bench  signal_roundtrip  N=1000");
    crate::log_u64("ktest: bench  cycles_mean=", total / N);

    cap_delete(th).ok();
    cap_delete(ping).ok();
    cap_delete(pong).ok();
    cap_delete(cs).ok();
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Run all Tier 3 benchmarks.
///
/// Called from `main.rs` after Tier 2 integration tests complete. Results are
/// logged directly via `klog`/`log_u64`; no PASS/FAIL counters are updated.
///
/// To add a benchmark: write a `bench_<name>` function and call it here.
pub fn run_all(ctx: &crate::TestContext)
{
    bench_null_syscall(ctx);
    bench_ipc_round_trip(ctx);
    bench_signal_roundtrip(ctx);
}
