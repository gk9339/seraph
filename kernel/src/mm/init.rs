// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/mm/init.rs

//! Physical memory initialization.
//!
//! Parses the boot-time memory map from [`BootInfo`], subtracts all reserved
//! regions (kernel image, init segments, boot modules, BootInfo struct), and
//! populates a caller-supplied [`BuddyAllocator`] with the surviving ranges.
//!
//! The allocator is passed by mutable reference rather than returned by value
//! because it is ~41 KiB — too large for the kernel's boot stack. The caller
//! should hold the allocator in static storage (BSS).

use core::mem::size_of;

use boot_protocol::{BootInfo, MemoryType};

use super::buddy::{BuddyAllocator, PAGE_SIZE};

/// Maximum usable physical ranges tracked during init. Memory maps are
/// typically small; 64 entries is generous for real hardware.
const MAX_RANGES: usize = 64;

/// Maximum exclusion regions. One per: kernel, BootInfo, each init segment
/// (up to 8), plus up to 16 boot modules — 32 is comfortably sufficient.
const MAX_EXCL: usize = 32;

/// Populate `alloc` with usable physical frames derived from `info`.
///
/// Filters for [`MemoryType::Usable`] entries, subtracts the kernel image,
/// init segments, boot modules, and the `BootInfo` struct itself, then feeds
/// the surviving page-aligned sub-ranges to `alloc`.
///
/// Calls [`crate::fatal`] and halts if no usable memory survives.
///
/// `alloc` must be a freshly-created (empty) allocator — this function does
/// not reset it first.
pub fn init_physical_memory(info: &BootInfo, alloc: &mut BuddyAllocator)
{
    let usable = collect_usable_ranges(info);
    let excl = collect_exclusions(info);

    for &(r_start, r_end) in usable.iter().filter(|&&(s, e)| s < e)
    {
        add_surviving_subranges(alloc, r_start, r_end, excl.as_slice());
    }

    if alloc.free_page_count() == 0
    {
        crate::fatal("no usable physical memory after exclusions");
    }
}

// ── Helper types ─────────────────────────────────────────────────────────────

/// A fixed-size list of physical address ranges.
struct RangeList<const N: usize>
{
    data: [(u64, u64); N],
    len: usize,
}

impl<const N: usize> RangeList<N>
{
    const fn new() -> Self
    {
        Self {
            data: [(0, 0); N],
            len: 0,
        }
    }

    fn push(&mut self, start: u64, end: u64)
    {
        if start < end && self.len < N
        {
            self.data[self.len] = (start, end);
            self.len += 1;
        }
    }

    fn as_slice(&self) -> &[(u64, u64)]
    {
        &self.data[..self.len]
    }

    fn iter(&self) -> core::slice::Iter<'_, (u64, u64)>
    {
        self.as_slice().iter()
    }
}

// ── Collection helpers ────────────────────────────────────────────────────────

/// Collect page-aligned usable ranges from the memory map.
fn collect_usable_ranges(info: &BootInfo) -> RangeList<MAX_RANGES>
{
    let mut ranges = RangeList::new();

    // SAFETY: Phase 0 validated entries is non-null and count > 0. The memory
    // map region is identity-mapped by the bootloader at handoff.
    let entries = unsafe {
        core::slice::from_raw_parts(info.memory_map.entries, info.memory_map.count as usize)
    };

    for entry in entries
    {
        if entry.memory_type != MemoryType::Usable
        {
            continue;
        }
        let start = align_up(entry.physical_base, PAGE_SIZE as u64);
        let end = align_down(entry.physical_base + entry.size, PAGE_SIZE as u64);
        ranges.push(start, end);
    }

    ranges
}

/// Build the exclusion list: all physical ranges that must not be allocated.
///
/// Exclusion boundaries are rounded outward (start down, end up) to page
/// granularity so no live data sits on a partially-excluded page.
fn collect_exclusions(info: &BootInfo) -> RangeList<MAX_EXCL>
{
    let mut excl = RangeList::new();

    let mut add = |start: u64, end: u64| {
        excl.push(
            align_down(start, PAGE_SIZE as u64),
            align_up(end, PAGE_SIZE as u64),
        );
    };

    // Kernel image.
    add(
        info.kernel_physical_base,
        info.kernel_physical_base + info.kernel_size,
    );

    // Init ELF segments (pre-parsed by the bootloader).
    let seg_count = info.init_image.segment_count as usize;
    for seg in &info.init_image.segments[..seg_count]
    {
        add(seg.phys_addr, seg.phys_addr + seg.size);
    }

    // Boot modules.
    if !info.modules.entries.is_null() && info.modules.count > 0
    {
        // SAFETY: entries is non-null and the region is identity-mapped.
        let modules = unsafe {
            core::slice::from_raw_parts(info.modules.entries, info.modules.count as usize)
        };
        for m in modules
        {
            add(m.physical_base, m.physical_base + m.size);
        }
    }

    // The BootInfo struct itself (pointed to by the kernel entry parameter).
    let boot_info_addr = info as *const BootInfo as u64;
    add(
        boot_info_addr,
        boot_info_addr + size_of::<BootInfo>() as u64,
    );

    // AP SIPI trampoline page (x86-64 SMP). Reported as Usable by the bootloader
    // (EfiBootServicesData → Usable), so without this exclusion the buddy
    // allocator would hand it out for IST stacks or heap, zeroing the trampoline
    // code that the BSP writes there during AP startup.
    if info.ap_trampoline_page != 0
    {
        add(
            info.ap_trampoline_page,
            info.ap_trampoline_page + PAGE_SIZE as u64,
        );
    }

    excl
}

// ── Range subtraction ─────────────────────────────────────────────────────────

/// Feed the sub-ranges of `[r_start, r_end)` that don't overlap any exclusion
/// in `excls` to the allocator.
fn add_surviving_subranges(
    alloc: &mut BuddyAllocator,
    r_start: u64,
    r_end: u64,
    excls: &[(u64, u64)],
)
{
    // `work` holds the set of ranges still available after applying each
    // exclusion in turn. Using a fixed-size array avoids heap allocation.
    let mut work = RangeList::<MAX_RANGES>::new();
    work.push(r_start, r_end);

    for &(e_start, e_end) in excls
    {
        let mut next = RangeList::<MAX_RANGES>::new();

        for &(w_start, w_end) in work.iter()
        {
            // Left sub-range: portion of [w_start, w_end) before the exclusion.
            let left_end = w_end.min(e_start);
            next.push(w_start, left_end);

            // Right sub-range: portion after the exclusion.
            let right_start = w_start.max(e_end);
            next.push(right_start, w_end);
        }

        work = next;
    }

    // Add all surviving sub-ranges to the allocator.
    for &(start, end) in work.iter()
    {
        if start < end
        {
            // SAFETY: These ranges are usable RAM not occupied by any live data,
            // confirmed by the exclusion subtraction above. The bootloader's
            // identity map covers them at this point in boot.
            unsafe { alloc.add_region(start, end) };
        }
    }
}

// ── Alignment utilities ───────────────────────────────────────────────────────

/// Round `val` up to the next multiple of `align`. `align` must be a power of two.
fn align_up(val: u64, align: u64) -> u64
{
    (val + align - 1) & !(align - 1)
}

/// Round `val` down to the nearest multiple of `align`. `align` must be a power of two.
fn align_down(val: u64, align: u64) -> u64
{
    val & !(align - 1)
}
