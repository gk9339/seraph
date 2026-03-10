// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/arch/x86_64/paging.rs

//! x86-64 four-level (PML4 → PDPT → PD → PT) page table operations.
//!
//! All page table frames must come from the BSS-resident pool supplied via
//! [`PoolState`]. Physical addresses of pool frames convert to virtual
//! addresses with the kernel VA/PA offset embedded in `PoolState`.
//!
//! # Index layout (48-bit canonical VA)
//! - Bits \[47:39\] → PML4 index  (512 entries × 512 GiB each)
//! - Bits \[38:30\] → PDPT index  (512 entries × 1 GiB each)
//! - Bits \[29:21\] → PD index    (512 entries × 2 MiB each)
//! - Bits \[20:12\] → PT index    (512 entries × 4 KiB each)

use crate::mm::paging::{PageFlags, PagingError, PoolState};

// ── PTE bit constants ─────────────────────────────────────────────────────────

/// Entry is valid; must be set for all live entries.
const PRESENT: u64 = 1 << 0;
/// Read/Write — 1 allows writes, 0 makes the mapping read-only.
const WRITABLE: u64 = 1 << 1;
/// Page Size (PS) — set in a PDE/PDPTE to make it a large-page leaf.
const LARGE_PAGE: u64 = 1 << 7;
/// No-Execute — blocks instruction fetch; requires IA32_EFER.NXE = 1.
const NO_EXECUTE: u64 = 1 << 63;
/// Mask extracting the physical page number from bits \[51:12\].
const PHYS_MASK: u64 = 0x000F_FFFF_FFFF_F000;

// ── PageTableEntry ────────────────────────────────────────────────────────────

/// A 64-bit x86-64 page table entry (PML4E, PDPTE, PDE, or PTE).
///
/// Transparent newtype over `u64`. Methods cover the three entry kinds:
/// table pointer, 4 KiB leaf page, and 2 MiB large-page leaf.
#[derive(Clone, Copy, Default)]
#[repr(transparent)]
pub struct PageTableEntry(pub u64);

impl PageTableEntry
{
    /// Construct a non-leaf (table pointer) entry pointing to `phys`.
    ///
    /// Sets P=1 and R/W=1 so the subordinate table is always writable.
    /// Clears NX so executable pages in the subtree are reachable.
    /// `phys` must be 4 KiB-aligned.
    pub fn new_table(phys: u64) -> Self
    {
        debug_assert!(phys & 0xFFF == 0, "table PA not 4 KiB-aligned");
        Self(PRESENT | WRITABLE | (phys & PHYS_MASK))
    }

    /// Construct a 4 KiB leaf page entry with `flags`.
    ///
    /// `phys` must be 4 KiB-aligned. `readable` has no effect on x86-64
    /// (all present entries are readable); included for cross-arch symmetry.
    pub fn new_page(phys: u64, flags: PageFlags) -> Self
    {
        debug_assert!(phys & 0xFFF == 0, "page PA not 4 KiB-aligned");
        let mut bits = PRESENT | (phys & PHYS_MASK);
        if flags.writable
        {
            bits |= WRITABLE;
        }
        if !flags.executable
        {
            bits |= NO_EXECUTE;
        }
        Self(bits)
    }

    /// Construct a 2 MiB large-page entry (PS bit set in a PDE) with `flags`.
    ///
    /// `phys` must be 2 MiB-aligned.
    pub fn new_large_page(phys: u64, flags: PageFlags) -> Self
    {
        debug_assert!(phys & 0x1F_FFFF == 0, "large page PA not 2 MiB-aligned");
        let mut bits = PRESENT | LARGE_PAGE | (phys & PHYS_MASK);
        if flags.writable
        {
            bits |= WRITABLE;
        }
        if !flags.executable
        {
            bits |= NO_EXECUTE;
        }
        Self(bits)
    }

    /// Return the physical address encoded in this entry (bits \[51:12\] × 4 KiB).
    pub fn phys_addr(self) -> u64
    {
        self.0 & PHYS_MASK
    }

    /// Return `true` if the Present bit is set.
    pub fn is_present(self) -> bool
    {
        self.0 & PRESENT != 0
    }
}

// ── VA index extraction ───────────────────────────────────────────────────────

/// PML4 index from a 64-bit VA (bits \[47:39\]).
pub fn pml4_index(va: u64) -> usize
{
    ((va >> 39) & 0x1FF) as usize
}

/// PDPT index from a 64-bit VA (bits \[38:30\]).
pub fn pdpt_index(va: u64) -> usize
{
    ((va >> 30) & 0x1FF) as usize
}

/// PD index from a 64-bit VA (bits \[29:21\]).
pub fn pd_index(va: u64) -> usize
{
    ((va >> 21) & 0x1FF) as usize
}

/// PT index from a 64-bit VA (bits \[20:12\]).
pub fn pt_index(va: u64) -> usize
{
    ((va >> 12) & 0x1FF) as usize
}

// ── Table frame access ────────────────────────────────────────────────────────

/// Reinterpret a 4 KiB pool frame as an array of 512 PTEs.
///
/// # Safety
/// `frame_va` must be the virtual address of a valid, writable, 4 KiB-aligned
/// pool frame. No other mutable reference to the same frame may exist.
unsafe fn table_at(frame_va: u64) -> &'static mut [PageTableEntry; 512]
{
    // SAFETY: contract stated in doc comment.
    unsafe { &mut *(frame_va as *mut [PageTableEntry; 512]) }
}

// ── Mapping functions ─────────────────────────────────────────────────────────

/// Map VA `virt` → PA `phys` as a 4 KiB page with `flags`.
///
/// Walks PML4 → PDPT → PD → PT, allocating missing intermediate tables from
/// `pool`. `root_va` is the virtual address of the root PML4 frame.
///
/// # Errors
/// `PagingError::OutOfFrames` if the pool cannot supply an intermediate frame.
pub fn map_page(
    root_va: u64,
    virt: u64,
    phys: u64,
    flags: PageFlags,
    pool: &mut PoolState,
) -> Result<(), PagingError>
{
    // SAFETY: root_va is a valid pool frame VA supplied by the orchestration layer.
    let pml4 = unsafe { table_at(root_va) };
    let pdpt_pa = walk_or_alloc(&mut pml4[pml4_index(virt)], pool)?;

    let pdpt = unsafe { table_at(pool.phys_to_virt(pdpt_pa)) };
    let pd_pa = walk_or_alloc(&mut pdpt[pdpt_index(virt)], pool)?;

    let pd = unsafe { table_at(pool.phys_to_virt(pd_pa)) };
    let pt_pa = walk_or_alloc(&mut pd[pd_index(virt)], pool)?;

    let pt = unsafe { table_at(pool.phys_to_virt(pt_pa)) };
    pt[pt_index(virt)] = PageTableEntry::new_page(phys, flags);
    Ok(())
}

/// Map VA `virt` → PA `phys` as a 2 MiB large page with `flags`.
///
/// Walks PML4 → PDPT → PD, allocating missing tables from `pool`, then
/// installs a large-page leaf at the PD level (no PT allocated).
///
/// # Errors
/// `PagingError::OutOfFrames` if the pool cannot supply an intermediate frame.
pub fn map_large_page(
    root_va: u64,
    virt: u64,
    phys: u64,
    flags: PageFlags,
    pool: &mut PoolState,
) -> Result<(), PagingError>
{
    let pml4 = unsafe { table_at(root_va) };
    let pdpt_pa = walk_or_alloc(&mut pml4[pml4_index(virt)], pool)?;

    let pdpt = unsafe { table_at(pool.phys_to_virt(pdpt_pa)) };
    let pd_pa = walk_or_alloc(&mut pdpt[pdpt_index(virt)], pool)?;

    let pd = unsafe { table_at(pool.phys_to_virt(pd_pa)) };
    pd[pd_index(virt)] = PageTableEntry::new_large_page(phys, flags);
    Ok(())
}

/// Return the child table physical address from `entry`, allocating a new
/// zeroed pool frame and installing it when `entry` is not present.
fn walk_or_alloc(entry: &mut PageTableEntry, pool: &mut PoolState) -> Result<u64, PagingError>
{
    if entry.is_present()
    {
        Ok(entry.phys_addr())
    }
    else
    {
        let (frame_va, frame_pa) = pool.alloc_frame()?;
        // Zero the new table (BSS frames start zeroed; explicit for safety).
        // SAFETY: frame_va is a freshly allocated, exclusively-owned pool frame.
        unsafe {
            core::ptr::write_bytes(frame_va as *mut u8, 0, 4096);
        }
        *entry = PageTableEntry::new_table(frame_pa);
        Ok(frame_pa)
    }
}

// ── Hardware operations ───────────────────────────────────────────────────────
// These functions use privileged instructions. They are excluded from unit
// test builds (they compile fine on x86-64 hosts but must never be called
// from user-space tests; the cfg gate prevents accidental invocation).

/// Activate the page tables rooted at `root_phys` by writing CR3.
///
/// The CPU immediately begins using the new tables. Any virtual address that
/// is not mapped in the new tables will fault.
///
/// # Safety
/// The tables must map:
/// - The currently executing kernel code at its virtual address.
/// - The active stack at its current virtual address.
/// - All data accessed immediately after this call.
#[cfg(not(test))]
pub unsafe fn activate(root_phys: u64)
{
    // SAFETY: caller guarantees completeness of the new tables (see doc comment).
    unsafe {
        core::arch::asm!(
            "mov cr3, {}",
            in(reg) root_phys,
            options(nostack),
        );
    }
}

/// Enable No-Execute by setting IA32_EFER.NXE (bit 11) via RDMSR/WRMSR.
///
/// Must be called before activating page tables that use the NX bit,
/// because bit 63 of a PTE is "reserved" when NXE = 0.
///
/// # Safety
/// Must execute at privilege level 0. Does not check CPUID; all QEMU
/// configurations and modern x86-64 hardware support NX.
#[cfg(not(test))]
pub unsafe fn enable_nx()
{
    /// IA32_EFER MSR address.
    const IA32_EFER: u32 = 0xC000_0080;
    /// No-Execute Enable bit.
    const NXE: u64 = 1 << 11;

    // SAFETY: ring-0 MSR read/write; caller guarantees privilege level.
    unsafe {
        let lo: u32;
        let hi: u32;
        core::arch::asm!(
            "rdmsr",
            in("ecx") IA32_EFER,
            out("eax") lo,
            out("edx") hi,
            options(nostack, nomem),
        );
        let efer = (((hi as u64) << 32) | lo as u64) | NXE;
        core::arch::asm!(
            "wrmsr",
            in("ecx") IA32_EFER,
            in("eax") (efer & 0xFFFF_FFFF) as u32,
            in("edx") (efer >> 32) as u32,
            options(nostack, nomem),
        );
    }
}

/// Read the current stack pointer (RSP).
///
/// Used before activating new page tables to determine which region to
/// identity-map for the boot stack.
pub fn read_stack_pointer() -> u64
{
    let sp: u64;
    // SAFETY: RSP is always readable at ring 0.
    unsafe {
        core::arch::asm!("mov {}, rsp", out(reg) sp, options(nostack, nomem));
    }
    sp
}

/// Read the current page table root physical address from CR3.
///
/// Returns the physical address of the active PML4 table. Strips the low
/// 12 bits (PCID and flags) per the CR3 layout specification.
///
/// # Safety
/// Must be called at ring 0.
#[cfg(not(test))]
pub unsafe fn read_root_phys() -> u64
{
    let cr3: u64;
    // SAFETY: reading CR3 is safe at ring 0.
    unsafe {
        core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nostack, nomem));
    }
    // Strip low 12 bits (PCID field / flags in no-PCID mode).
    cr3 & !0xFFF
}

/// Map a single 4 KiB user page `virt` → `phys` in the page table rooted at
/// `root_virt`, allocating missing intermediate frames from `allocator`.
///
/// Unlike `map_page` (which uses a BSS pool), this function allocates
/// intermediate page table frames dynamically from the buddy allocator.
/// Used for building user address spaces after Phase 4 (heap active).
///
/// # Errors
/// Returns `Err(())` if the buddy allocator is exhausted.
///
/// # Safety
/// `root_virt` must be the direct-map virtual address of a valid 4 KiB PML4
/// frame. `virt` must be in the lower (user) half. `phys` must be 4 KiB-aligned.
#[cfg(not(test))]
pub unsafe fn map_user_page(
    root_virt: u64,
    virt: u64,
    phys: u64,
    flags: crate::mm::paging::PageFlags,
    allocator: &mut crate::mm::BuddyAllocator,
) -> Result<(), ()>
{
    use crate::mm::paging::phys_to_virt;

    // SAFETY: root_virt is a valid 4 KiB page table frame.
    let pml4 = unsafe { table_at(root_virt) };

    let pdpt_pa = user_walk_or_alloc(&mut pml4[pml4_index(virt)], allocator)?;
    let pdpt = unsafe { table_at(phys_to_virt(pdpt_pa)) };

    let pd_pa = user_walk_or_alloc(&mut pdpt[pdpt_index(virt)], allocator)?;
    let pd = unsafe { table_at(phys_to_virt(pd_pa)) };

    let pt_pa = user_walk_or_alloc(&mut pd[pd_index(virt)], allocator)?;
    let pt = unsafe { table_at(phys_to_virt(pt_pa)) };

    // Set USER bit (bit 2) so ring-3 code can access the page.
    const USER: u64 = 1 << 2;
    let mut pte = PageTableEntry::new_page(phys, flags);
    pte.0 |= USER;
    pt[pt_index(virt)] = pte;

    Ok(())
}

/// Walk an existing page table entry or allocate a new child frame from the
/// buddy allocator.
///
/// Used by `map_user_page` in place of `walk_or_alloc` (which uses PoolState).
#[cfg(not(test))]
fn user_walk_or_alloc(
    entry: &mut PageTableEntry,
    allocator: &mut crate::mm::BuddyAllocator,
) -> Result<u64, ()>
{
    use crate::mm::paging::phys_to_virt;
    use crate::mm::PAGE_SIZE;

    if entry.is_present()
    {
        return Ok(entry.phys_addr());
    }

    let frame_pa = allocator.alloc(0).ok_or(())?;
    let frame_va = phys_to_virt(frame_pa);

    // Zero the new table.
    // SAFETY: frame_va is an exclusively-owned direct-map kernel address.
    unsafe {
        core::ptr::write_bytes(frame_va as *mut u8, 0, PAGE_SIZE);
    }

    // Set USER bit so lower-level tables are accessible from ring 3.
    const USER: u64 = 1 << 2;
    let mut table_pte = PageTableEntry::new_table(frame_pa);
    table_pte.0 |= USER;
    *entry = table_pte;

    Ok(frame_pa)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests
{
    use super::*;
    use crate::mm::paging::DIRECT_MAP_BASE;

    // ── PTE construction ──────────────────────────────────────────────────────

    #[test]
    fn new_table_sets_present_and_writable()
    {
        let pte = PageTableEntry::new_table(0x1000);
        assert!(pte.is_present());
        assert!(pte.0 & WRITABLE != 0);
    }

    #[test]
    fn new_table_clears_no_execute()
    {
        let pte = PageTableEntry::new_table(0x1000);
        assert!(pte.0 & NO_EXECUTE == 0);
    }

    #[test]
    fn new_page_rx_sets_present_clears_writable_clears_nx()
    {
        let flags = PageFlags {
            readable: true,
            writable: false,
            executable: true,
        };
        let pte = PageTableEntry::new_page(0x2000, flags);
        assert!(pte.is_present());
        assert_eq!(pte.0 & WRITABLE, 0);
        assert_eq!(pte.0 & NO_EXECUTE, 0);
    }

    #[test]
    fn new_page_rw_sets_present_sets_writable_sets_nx()
    {
        let flags = PageFlags {
            readable: true,
            writable: true,
            executable: false,
        };
        let pte = PageTableEntry::new_page(0x3000, flags);
        assert!(pte.is_present());
        assert!(pte.0 & WRITABLE != 0);
        assert!(pte.0 & NO_EXECUTE != 0);
    }

    #[test]
    fn new_large_page_sets_ps_bit()
    {
        let flags = PageFlags {
            readable: true,
            writable: true,
            executable: false,
        };
        let pte = PageTableEntry::new_large_page(0x20_0000, flags);
        assert!(pte.0 & LARGE_PAGE != 0);
    }

    #[test]
    fn phys_addr_masks_out_flag_bits()
    {
        let pte = PageTableEntry::new_table(0xDEAD_B000);
        assert_eq!(pte.phys_addr(), 0xDEAD_B000);
    }

    #[test]
    fn is_present_false_for_zero_entry()
    {
        let pte = PageTableEntry(0);
        assert!(!pte.is_present());
    }

    // ── VA index extraction ───────────────────────────────────────────────────

    #[test]
    fn direct_map_base_pml4_index_is_256()
    {
        assert_eq!(pml4_index(DIRECT_MAP_BASE), 256);
    }

    #[test]
    fn direct_map_base_pdpt_and_pd_index_are_zero()
    {
        assert_eq!(pdpt_index(DIRECT_MAP_BASE), 0);
        assert_eq!(pd_index(DIRECT_MAP_BASE), 0);
    }

    #[test]
    fn kernel_vbase_indices()
    {
        // Kernel image at 0xFFFF_FFFF_8000_0000: PML4=511, PDPT=510, PD=0.
        let kv: u64 = 0xFFFF_FFFF_8000_0000;
        assert_eq!(pml4_index(kv), 511);
        assert_eq!(pdpt_index(kv), 510);
        assert_eq!(pd_index(kv), 0);
    }

    #[test]
    fn pt_index_extracts_bits_20_to_12()
    {
        // VA = 0x0000_0000_0012_3456: bits [20:12] = 0x123 = 291
        assert_eq!(pt_index(0x0000_0000_0012_3000), 0x123);
    }
}
