// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// ktest/src/unit/sysinfo.rs

//! Tier 1 tests for system info and debug log syscalls.
//!
//! Covers: `SYS_SYSTEM_INFO` (all `SystemInfoType` variants and unknown
//! discriminant), `SYS_DEBUG_LOG`.

use syscall::system_info;
use syscall_abi::{SyscallError, SystemInfoType, KERNEL_VERSION};

use crate::{TestContext, TestResult};

// ── SYS_SYSTEM_INFO ───────────────────────────────────────────────────────────

/// `system_info(KernelVersion)` returns the expected version constant.
///
/// The kernel version is `0.0.1` (packed as `1`). See `KERNEL_VERSION` in
/// `abi/syscall/src/lib.rs` for the encoding details.
pub fn kernel_version(_ctx: &TestContext) -> TestResult
{
    let ver = system_info(SystemInfoType::KernelVersion as u64)
        .map_err(|_| "system_info(KernelVersion) failed")?;
    if ver != KERNEL_VERSION
    {
        return Err("system_info(KernelVersion) returned unexpected value");
    }
    // Log the version in "vMAJOR.MINOR.PATCH" format for visibility.
    crate::log_version("ktest: kernel version ", ver);
    Ok(())
}

/// `system_info(CpuCount)` returns a value ≥ 1.
///
/// The exact count depends on the QEMU `-smp` setting and whether SMP has
/// been initialised. Until WSMP (SMP) the kernel reports 1 regardless.
pub fn cpu_count(_ctx: &TestContext) -> TestResult
{
    let cpus =
        system_info(SystemInfoType::CpuCount as u64).map_err(|_| "system_info(CpuCount) failed")?;
    if cpus == 0
    {
        return Err("system_info(CpuCount) returned 0 (expected at least 1)");
    }
    crate::log_u64("ktest: CpuCount=", cpus);
    Ok(())
}

/// `system_info(FreeFrames)` and `system_info(TotalFrames)` return consistent values.
///
/// `FreeFrames` must be > 0 and ≤ `TotalFrames`. `TotalFrames` must be > 0.
pub fn frame_counts(_ctx: &TestContext) -> TestResult
{
    let free = system_info(SystemInfoType::FreeFrames as u64)
        .map_err(|_| "system_info(FreeFrames) failed")?;
    let total = system_info(SystemInfoType::TotalFrames as u64)
        .map_err(|_| "system_info(TotalFrames) failed")?;

    if free == 0
    {
        return Err("system_info(FreeFrames) returned 0");
    }
    if total == 0
    {
        return Err("system_info(TotalFrames) returned 0");
    }
    if free > total
    {
        return Err("FreeFrames > TotalFrames (inconsistent memory accounting)");
    }
    crate::log_u64("ktest: FreeFrames=", free);
    crate::log_u64("ktest: TotalFrames=", total);
    Ok(())
}

/// `system_info(PageSize)` must return 4096.
pub fn page_size(_ctx: &TestContext) -> TestResult
{
    let sz =
        system_info(SystemInfoType::PageSize as u64).map_err(|_| "system_info(PageSize) failed")?;
    if sz != 4096
    {
        return Err("system_info(PageSize) did not return 4096");
    }
    Ok(())
}

/// `system_info(BootProtocolVersion)` must return the current protocol version (4).
pub fn boot_protocol_version(_ctx: &TestContext) -> TestResult
{
    let bpv = system_info(SystemInfoType::BootProtocolVersion as u64)
        .map_err(|_| "system_info(BootProtocolVersion) failed")?;
    if bpv != 4
    {
        return Err("system_info(BootProtocolVersion) did not return 4");
    }
    Ok(())
}

/// `system_info` with an unknown discriminant returns `InvalidArgument`.
pub fn unknown_kind_err(_ctx: &TestContext) -> TestResult
{
    let err = system_info(0xFFFF_FFFF);
    if err != Err(SyscallError::InvalidArgument as i64)
    {
        return Err("system_info(unknown kind) did not return InvalidArgument");
    }
    Ok(())
}

/// `system_info(CpuCount)` returns ≥ 2 when the kernel was booted with SMP.
///
/// Requires QEMU `-smp N` with N ≥ 2. Skips with a log message rather than
/// failing if only one CPU is online, so single-CPU runs stay green.
pub fn cpu_count_smp(_ctx: &TestContext) -> TestResult
{
    let cpus =
        system_info(SystemInfoType::CpuCount as u64).map_err(|_| "system_info(CpuCount) failed")?;
    if cpus < 2
    {
        crate::klog("ktest: sysinfo::cpu_count_smp SKIP (boot with -smp N for SMP test)");
        return Ok(());
    }
    crate::log_u64("ktest: SMP CpuCount=", cpus);
    Ok(())
}

// ── SYS_DEBUG_LOG ─────────────────────────────────────────────────────────────

/// `debug_log` accepts a valid UTF-8 string without error.
///
/// This is a development scaffold syscall; this test just confirms it does not
/// unexpectedly fail. All other tests already rely on it working.
pub fn debug_log_works(_ctx: &TestContext) -> TestResult
{
    syscall::debug_log("ktest: sysinfo::debug_log self-test")
        .map_err(|_| "debug_log returned an error")?;
    Ok(())
}
