// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// ktest/src/integration/memory_lifecycle.rs

//! Integration: frame split → map → protect → unmap.
//!
//! Exercises the full memory management lifecycle as a single coherent scenario,
//! verifying the address-space state with `aspace_query` at each step.
//!
//! Scans the initial segment frame caps for one splittable at a page boundary;
//! skips if none qualify (e.g. all segments fit in one page).
//!
//! Syscalls exercised in sequence:
//!   `frame_split` → `mem_map` → `aspace_query` (expect mapped) →
//!   `mem_protect` → `mem_unmap` → `aspace_query` (expect not mapped) →
//!   `mem_unmap` (idempotent check)

use syscall::{aspace_query, frame_split, mem_map, mem_protect, mem_unmap};

use crate::{TestContext, TestResult};

const TEST_VA: u64 = 0x4800_0000; // 1.125 GiB — distinct from unit/mm.rs TEST_VA.
const PAGE: u64 = 0x1000;

pub fn run(ctx: &TestContext) -> TestResult
{
    // ── 1. Find a splittable frame cap. ──────────────────────────────────────
    //
    // Segment frame caps live at aspace_cap+1 .. aspace_cap+segment_count.
    // We don't know which segments are large enough to split at PAGE, so scan
    // all of them (typically 3: text, rodata, data/bss) and use the first one
    // that frame_split accepts. If none are splittable, skip the test.
    let mut split_result = None;
    for offset in 1..=3u32
    {
        let cap = ctx.aspace_cap + offset;
        if let Ok(pair) = frame_split(cap, PAGE)
        {
            split_result = Some(pair);
            break;
        }
    }
    let (frame_a, frame_b) = match split_result
    {
        Some(pair) => pair,
        None =>
        {
            crate::klog("ktest: integration::memory_lifecycle SKIP (no splittable frame cap)");
            return Ok(());
        }
    };

    // ── 2. Map frame_a (one page) at TEST_VA. ────────────────────────────────
    mem_map(frame_a, ctx.aspace_cap, TEST_VA, 0, 1)
        .map_err(|_| "integration::memory_lifecycle: mem_map failed")?;

    // ── 3. Verify the mapping via aspace_query. ───────────────────────────────
    let phys_after_map = aspace_query(ctx.aspace_cap, TEST_VA)
        .map_err(|_| "integration::memory_lifecycle: aspace_query after map failed")?;
    if phys_after_map == 0 || phys_after_map & 0xFFF != 0
    {
        return Err("integration::memory_lifecycle: aspace_query returned invalid phys after map");
    }

    // ── 4. Change protection to read-only. ───────────────────────────────────
    mem_protect(frame_a, ctx.aspace_cap, TEST_VA, 1, 0)
        .map_err(|_| "integration::memory_lifecycle: mem_protect (read-only) failed")?;

    // ── 5. Protect an unmapped VA — must fail. ────────────────────────────────
    let protect_err = mem_protect(frame_a, ctx.aspace_cap, TEST_VA + 0x10_0000, 1, 0);
    if protect_err.is_ok()
    {
        return Err("integration::memory_lifecycle: mem_protect on unmapped VA should fail");
    }

    // ── 6. Unmap the page. ────────────────────────────────────────────────────
    mem_unmap(ctx.aspace_cap, TEST_VA, 1)
        .map_err(|_| "integration::memory_lifecycle: mem_unmap failed")?;

    // ── 7. Verify the page is no longer mapped. ───────────────────────────────
    let query_after_unmap = aspace_query(ctx.aspace_cap, TEST_VA);
    if query_after_unmap.is_ok()
    {
        return Err(
            "integration::memory_lifecycle: aspace_query succeeded after unmap (expected error)",
        );
    }

    // ── 8. Second unmap must be a no-op (not an error). ──────────────────────
    mem_unmap(ctx.aspace_cap, TEST_VA, 1)
        .map_err(|_| "integration::memory_lifecycle: idempotent mem_unmap failed")?;

    // Cleanup: delete the split child frame caps.
    syscall::cap_delete(frame_a)
        .map_err(|_| "integration::memory_lifecycle: cap_delete frame_a failed")?;
    syscall::cap_delete(frame_b)
        .map_err(|_| "integration::memory_lifecycle: cap_delete frame_b failed")?;

    Ok(())
}
