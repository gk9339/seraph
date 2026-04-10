// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// ktest/src/unit/mm.rs

//! Tier 1 tests for memory management syscalls.
//!
//! Covers: `SYS_FRAME_SPLIT`, `SYS_MEM_MAP`, `SYS_MEM_UNMAP`,
//! `SYS_MEM_PROTECT`, `SYS_ASPACE_QUERY`.
//!
//! Frame cap layout after `aspace_cap` (as provided by the kernel/bootloader):
//!   `aspace_cap + 1` — TEXT segment frame (MAP | EXECUTE)
//!   `aspace_cap + 2` — RODATA segment frame (MAP)
//!   `aspace_cap + 3` — BSS/DATA segment frame (MAP | WRITE)
//!
//! `frame_split` consumes the RODATA frame (`aspace_cap + 2`). TEXT and BSS
//! frames are left intact for the tests that use them directly.

use syscall::{aspace_query, mem_map, mem_unmap};

use crate::{TestContext, TestResult};

/// Safe test virtual address: 1 GiB. Well above ktest's load address and stack.
/// Used consistently across mm tests to avoid mapping conflicts.
const TEST_VA: u64 = 0x4000_0000;

// ── SYS_FRAME_SPLIT ───────────────────────────────────────────────────────────

/// `frame_split` divides a multi-page frame into two non-overlapping children.
///
/// Uses the RODATA frame (`aspace_cap + 2`) which spans multiple pages.
/// The split consumes the original cap and returns two new caps, both deleted
/// as cleanup. TEXT and BSS frames are intentionally left untouched.
pub fn frame_split(ctx: &TestContext) -> TestResult
{
    const PAGE: u64 = 0x1000;
    // RODATA frame spans multiple pages; always splittable at one page boundary.
    let rodata_cap = ctx.aspace_cap + 2;
    let (a, b) =
        syscall::frame_split(rodata_cap, PAGE).map_err(|_| "frame_split failed on RODATA frame")?;

    if a == b
    {
        return Err("frame_split returned identical slot indices for both halves");
    }

    // Clean up — delete both child frames (original cap is now gone).
    syscall::cap_delete(a).map_err(|_| "cap_delete frame_a failed")?;
    syscall::cap_delete(b).map_err(|_| "cap_delete frame_b failed")?;
    Ok(())
}

// ── SYS_MEM_MAP / SYS_MEM_UNMAP ──────────────────────────────────────────────

/// `mem_map` maps a frame page into the address space; `mem_unmap` removes it.
///
/// Allocates a frame from the pool, maps it, verifies via `aspace_query`,
/// then unmaps and returns the frame to the pool.
pub fn mem_map_unmap(ctx: &TestContext) -> TestResult
{
    let mut frame = crate::frame_pool::FrameGuard::new(ctx.aspace_cap)
        .ok_or("mem_map_unmap: frame pool exhausted")?;

    // Map one page at TEST_VA, offset 0 within the frame.
    frame.map(TEST_VA).map_err(|_| "mem_map failed")?;

    // Verify the mapping appears in the address space.
    let phys =
        aspace_query(ctx.aspace_cap, TEST_VA).map_err(|_| "aspace_query after mem_map failed")?;
    if phys == 0 || phys & 0xFFF != 0
    {
        return Err("aspace_query returned invalid physical address after mem_map");
    }

    // FrameGuard drop unmaps and returns frame to pool.
    Ok(())
}

// ── SYS_MEM_PROTECT ───────────────────────────────────────────────────────────

/// `mem_protect` changes permission flags on an existing mapping.
///
/// Maps a frame page, sets it to read-only (prot = 0: no WRITE, no EXECUTE),
/// then unmaps. Verifying that a write actually faults requires a userspace
/// fault handler (deferred).
pub fn mem_protect(ctx: &TestContext) -> TestResult
{
    let mut frame = crate::frame_pool::FrameGuard::new(ctx.aspace_cap)
        .ok_or("mem_protect: frame pool exhausted")?;

    frame.map(TEST_VA).map_err(|_| "mem_map for protect test failed")?;

    // prot = 0: read-only. Always valid regardless of frame rights.
    syscall::mem_protect(frame.cap(), ctx.aspace_cap, TEST_VA, 1, 0)
        .map_err(|_| "mem_protect (read-only) failed")?;

    // FrameGuard drop unmaps and returns frame to pool.
    Ok(())
}

// ── SYS_MEM_PROTECT negative ──────────────────────────────────────────────────

/// `mem_protect` on an unmapped virtual address must return an error.
pub fn mem_protect_unmapped_err(ctx: &TestContext) -> TestResult
{
    let frame = crate::frame_pool::alloc()
        .ok_or("mem_protect_unmapped_err: frame pool exhausted")?;
    // 0x1000_0000 is not mapped by ktest.
    let unmapped_va = 0x1000_0000u64;
    let err = syscall::mem_protect(frame, ctx.aspace_cap, unmapped_va, 1, 0);

    // SAFETY: frame was allocated from pool and never mapped.
    unsafe { crate::frame_pool::free(frame) };

    if err.is_ok()
    {
        return Err("mem_protect on unmapped VA should fail");
    }
    Ok(())
}

// ── SYS_MEM_UNMAP idempotent ──────────────────────────────────────────────────

/// Unmapping an already-unmapped VA is a no-op, not an error.
pub fn mem_unmap_idempotent(ctx: &TestContext) -> TestResult
{
    let mut frame = crate::frame_pool::FrameGuard::new(ctx.aspace_cap)
        .ok_or("mem_unmap_idempotent: frame pool exhausted")?;

    frame.map(TEST_VA).map_err(|_| "mem_map for idempotent-unmap test failed")?;
    mem_unmap(ctx.aspace_cap, TEST_VA, 1).map_err(|_| "first mem_unmap failed")?;
    // Second unmap of the same range must succeed (no-op).
    mem_unmap(ctx.aspace_cap, TEST_VA, 1).map_err(|_| "second mem_unmap (idempotent) failed")?;

    // FrameGuard drop will try to unmap again (third time) — also idempotent.
    Ok(())
}

// ── SYS_ASPACE_QUERY ─────────────────────────────────────────────────────────

/// `aspace_query` returns the physical address for a mapped page.
///
/// ktest's own `_start` page is always mapped R-X; use it as a stable target.
pub fn aspace_query_mapped(ctx: &TestContext) -> TestResult
{
    extern "C" {
        fn _start();
    }
    let code_va = (_start as *const () as u64) & !0xFFF;
    let phys =
        aspace_query(ctx.aspace_cap, code_va).map_err(|_| "aspace_query on _start page failed")?;
    if phys == 0 || phys & 0xFFF != 0
    {
        return Err("aspace_query returned non-page-aligned or zero physical address");
    }
    Ok(())
}

/// `aspace_query` on an unmapped virtual address must return an error.
pub fn aspace_query_unmapped_err(ctx: &TestContext) -> TestResult
{
    // 0x7000_0000_0000 is never mapped in ktest's address space.
    let err = aspace_query(ctx.aspace_cap, 0x7000_0000_0000u64);
    if err.is_ok()
    {
        return Err("aspace_query on unmapped VA should fail");
    }
    Ok(())
}

// ── SYS_MEM_MAP negative ──────────────────────────────────────────────────────

/// `mem_map` with a non-page-aligned virtual address must return an error.
pub fn mem_map_unaligned_vaddr_err(ctx: &TestContext) -> TestResult
{
    let frame = crate::frame_pool::alloc()
        .ok_or("mem_map_unaligned_vaddr_err: frame pool exhausted")?;
    let err = mem_map(frame, ctx.aspace_cap, TEST_VA + 1, 0, 1);

    // SAFETY: frame was allocated from pool and never successfully mapped.
    unsafe { crate::frame_pool::free(frame) };

    if err.is_ok()
    {
        return Err("mem_map with unaligned vaddr should fail");
    }
    Ok(())
}

/// `mem_map` targeting the kernel virtual address half must return an error.
///
/// On both x86-64 and RISC-V Sv48, `0xFFFF_8000_0000_0000` is in the kernel half.
pub fn mem_map_kernel_half_err(ctx: &TestContext) -> TestResult
{
    let frame = crate::frame_pool::alloc()
        .ok_or("mem_map_kernel_half_err: frame pool exhausted")?;
    let kernel_va: u64 = 0xFFFF_8000_0000_0000;
    let err = mem_map(frame, ctx.aspace_cap, kernel_va, 0, 1);

    // SAFETY: frame was allocated from pool and never successfully mapped.
    unsafe { crate::frame_pool::free(frame) };

    if err.is_ok()
    {
        return Err("mem_map into kernel address space should fail");
    }
    Ok(())
}

// ── SYS_FRAME_SPLIT negative ──────────────────────────────────────────────────

/// `frame_split` at offset 0 must return an error (left half would be empty).
pub fn frame_split_at_zero_err(_ctx: &TestContext) -> TestResult
{
    let frame = crate::frame_pool::alloc()
        .ok_or("frame_split_at_zero_err: frame pool exhausted")?;
    let err = syscall::frame_split(frame, 0);

    // If split fails (expected), frame cap is still valid, so return it to pool.
    // SAFETY: frame was allocated from pool; split failed so it's still intact.
    unsafe { crate::frame_pool::free(frame) };

    if err.is_ok()
    {
        return Err("frame_split at offset 0 should fail (zero-size left half)");
    }
    Ok(())
}

// ── SYS_MEM_PROTECT negative ──────────────────────────────────────────────────

/// `mem_protect` requesting permissions beyond the frame cap's rights must fail.
///
/// Pool frames have `MAP|WRITE|EXECUTE` rights. To test insufficient rights,
/// we'd need to derive an attenuated cap. For now, this test verifies that
/// `mem_protect` with valid rights on a mapped page succeeds (sanity check).
///
/// A true negative test for insufficient rights would require capability
/// derivation with attenuation, which is tested in `cap::derive_attenuation`.
pub fn mem_protect_exceeds_cap_rights_err(ctx: &TestContext) -> TestResult
{
    // Use a VA distinct from TEST_VA=0x4000_0000 to avoid conflicts.
    const PROTECT_TEST_VA: u64 = 0x4100_0000;

    // Use the TEXT segment frame which has MAP|EXECUTE but no WRITE.
    let text_frame = ctx.aspace_cap + 1;

    mem_map(text_frame, ctx.aspace_cap, PROTECT_TEST_VA, 0, 1)
        .map_err(|_| "mem_map for protect-rights test failed")?;

    // prot_bits = 2 means WRITE; TEXT cap has no WRITE right (only MAP|EXECUTE).
    let err = syscall::mem_protect(text_frame, ctx.aspace_cap, PROTECT_TEST_VA, 1, 2);

    // Always unmap regardless of protect result.
    mem_unmap(ctx.aspace_cap, PROTECT_TEST_VA, 1).ok();

    if err.is_ok()
    {
        return Err("mem_protect with WRITE on MAP|EXECUTE cap should fail (InsufficientRights)");
    }
    Ok(())
}
