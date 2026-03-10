// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/arch/riscv64/paging.rs

//! RISC-V RV64GC Sv48 four-level page table operations.
//!
//! Mirrors the x86-64 interface. All page table frames come from the
//! BSS-resident pool supplied via [`PoolState`].
//!
//! # Sv48 index layout (48-bit VA, 4 levels)
//! - Bits \[47:39\] → VPN\[3\] — root level (512 entries × 512 GiB each)
//! - Bits \[38:30\] → VPN\[2\] — level 2     (512 entries × 1 GiB each)
//! - Bits \[29:21\] → VPN\[1\] — level 1     (512 entries × 2 MiB each)
//! - Bits \[20:12\] → VPN\[0\] — leaf level  (512 entries × 4 KiB each)
//!
//! # PTE layout
//! Bits \[53:10\]: PPN (physical page number, 44 bits).
//! Bit 0: V (Valid). Bit 1: R (Read). Bit 2: W (Write). Bit 3: X (Execute).
//! Non-leaf: V=1, R=0, W=0, X=0. Leaf: V=1, at least one of R/W/X set.
//! A megapage (2 MiB) is a leaf installed at level 1 (VPN\[1\]).

use crate::mm::paging::{PageFlags, PagingError, PoolState};

// ── PTE bit constants ─────────────────────────────────────────────────────────

/// Entry is valid.
const VALID: u64 = 1 << 0;
/// Read permission.
const READ: u64 = 1 << 1;
/// Write permission.
const WRITE: u64 = 1 << 2;
/// Execute permission.
const EXECUTE: u64 = 1 << 3;
/// Accessed — must be pre-set in leaf PTEs.
///
/// Some RISC-V implementations (including QEMU TCG) do not perform hardware
/// A-bit updates and instead raise a page fault when A=0 is encountered.
/// Pre-setting A=1 avoids this on first access.
const ACCESSED: u64 = 1 << 6;
/// Dirty — must be pre-set in writable leaf PTEs (same rationale as ACCESSED).
const DIRTY: u64 = 1 << 7;
/// PPN field mask: bits \[53:10\], representing `(phys >> 12) << 10`.
const PPN_MASK: u64 = 0x003F_FFFF_FFFF_FC00;

// ── PageTableEntry ────────────────────────────────────────────────────────────

/// A 64-bit RISC-V Sv48 page table entry (PTE).
#[derive(Clone, Copy, Default)]
#[repr(transparent)]
pub struct PageTableEntry(pub u64);

impl PageTableEntry
{
    /// Construct a non-leaf entry pointing to a child table at `phys`.
    ///
    /// V=1, R=0, W=0, X=0. PPN holds `phys >> 12`. `phys` must be 4 KiB-aligned.
    pub fn new_table(phys: u64) -> Self
    {
        debug_assert!(phys & 0xFFF == 0, "table PA not 4 KiB-aligned");
        // PPN in bits [53:10] = (phys >> 12) << 10.
        Self(VALID | ((phys >> 2) & PPN_MASK))
    }

    /// Construct a 4 KiB leaf page entry with `flags`.
    ///
    /// `phys` must be 4 KiB-aligned.
    pub fn new_page(phys: u64, flags: PageFlags) -> Self
    {
        debug_assert!(phys & 0xFFF == 0, "page PA not 4 KiB-aligned");
        // ACCESSED must be pre-set: QEMU TCG raises a page fault on A=0 rather
        // than setting it in hardware. DIRTY is pre-set for writable pages for
        // the same reason.
        let mut bits = VALID | ACCESSED | ((phys >> 2) & PPN_MASK);
        if flags.readable
        {
            bits |= READ;
        }
        if flags.writable
        {
            bits |= WRITE | DIRTY;
        }
        if flags.executable
        {
            bits |= EXECUTE;
        }
        Self(bits)
    }

    /// Construct a 2 MiB megapage entry (leaf at VPN\[1\] level) with `flags`.
    ///
    /// `phys` must be 2 MiB-aligned.
    pub fn new_large_page(phys: u64, flags: PageFlags) -> Self
    {
        debug_assert!(phys & 0x1F_FFFF == 0, "large page PA not 2 MiB-aligned");
        // Same encoding as new_page; the "large" nature is conveyed by level.
        Self::new_page(phys, flags)
    }

    /// Return the physical address encoded in this entry.
    ///
    /// Extracts PPN from bits \[53:10\] and shifts left by 12.
    pub fn phys_addr(self) -> u64
    {
        // PPN is (bits & PPN_MASK) >> 10, then PA = PPN << 12 = PPN << 12.
        // Combined: (bits & PPN_MASK) >> 10 << 12 = (bits & PPN_MASK) << 2.
        (self.0 & PPN_MASK) << 2
    }

    /// Return `true` if the Valid bit is set.
    pub fn is_present(self) -> bool
    {
        self.0 & VALID != 0
    }
}

// ── VA index extraction ───────────────────────────────────────────────────────

/// VPN\[3\] (root) index from a VA (bits \[47:39\]).
pub fn vpn3_index(va: u64) -> usize
{
    ((va >> 39) & 0x1FF) as usize
}

/// VPN\[2\] index from a VA (bits \[38:30\]).
pub fn vpn2_index(va: u64) -> usize
{
    ((va >> 30) & 0x1FF) as usize
}

/// VPN\[1\] index from a VA (bits \[29:21\]).
pub fn vpn1_index(va: u64) -> usize
{
    ((va >> 21) & 0x1FF) as usize
}

/// VPN\[0\] (leaf) index from a VA (bits \[20:12\]).
pub fn vpn0_index(va: u64) -> usize
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
    // SAFETY: root_va is a valid pool frame VA.
    let root = unsafe { table_at(root_va) };
    let l2_pa = walk_or_alloc(&mut root[vpn3_index(virt)], pool)?;

    let l2 = unsafe { table_at(pool.phys_to_virt(l2_pa)) };
    let l1_pa = walk_or_alloc(&mut l2[vpn2_index(virt)], pool)?;

    let l1 = unsafe { table_at(pool.phys_to_virt(l1_pa)) };
    let l0_pa = walk_or_alloc(&mut l1[vpn1_index(virt)], pool)?;

    let l0 = unsafe { table_at(pool.phys_to_virt(l0_pa)) };
    l0[vpn0_index(virt)] = PageTableEntry::new_page(phys, flags);
    Ok(())
}

/// Map VA `virt` → PA `phys` as a 2 MiB megapage with `flags`.
///
/// Installs a leaf entry at the VPN\[1\] level; no VPN\[0\] table is allocated.
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
    let root = unsafe { table_at(root_va) };
    let l2_pa = walk_or_alloc(&mut root[vpn3_index(virt)], pool)?;

    let l2 = unsafe { table_at(pool.phys_to_virt(l2_pa)) };
    let l1_pa = walk_or_alloc(&mut l2[vpn2_index(virt)], pool)?;

    let l1 = unsafe { table_at(pool.phys_to_virt(l1_pa)) };
    l1[vpn1_index(virt)] = PageTableEntry::new_large_page(phys, flags);
    Ok(())
}

/// Return the child table physical address from `entry`, allocating and
/// zeroing a new pool frame when the entry is not present.
fn walk_or_alloc(entry: &mut PageTableEntry, pool: &mut PoolState) -> Result<u64, PagingError>
{
    if entry.is_present()
    {
        Ok(entry.phys_addr())
    }
    else
    {
        let (frame_va, frame_pa) = pool.alloc_frame()?;
        // SAFETY: frame_va is a freshly allocated, exclusively-owned pool frame.
        unsafe {
            core::ptr::write_bytes(frame_va as *mut u8, 0, 4096);
        }
        *entry = PageTableEntry::new_table(frame_pa);
        Ok(frame_pa)
    }
}

// ── Hardware operations ───────────────────────────────────────────────────────

/// Activate Sv48 paging by writing `satp` and issuing `sfence.vma`.
///
/// `satp` encoding: mode 9 (Sv48) in bits \[63:60\], ASID 0, root PPN in
/// bits \[43:0\].
///
/// # Safety
/// The tables must map the currently executing code and active stack.
#[cfg(not(test))]
pub unsafe fn activate(root_phys: u64)
{
    let satp = (9u64 << 60) | (root_phys >> 12);
    // SAFETY: caller guarantees the new tables are complete.
    unsafe {
        core::arch::asm!(
            "csrw satp, {}",
            "sfence.vma zero, zero",
            in(reg) satp,
            options(nostack),
        );
    }
}

/// No-op on RISC-V: the XN/NX mechanism is always available via PTE X bit.
#[cfg(not(test))]
pub unsafe fn enable_nx() {}

/// Read the current stack pointer (sp register).
pub fn read_stack_pointer() -> u64
{
    let sp: u64;
    // SAFETY: sp is always accessible in S-mode.
    unsafe {
        core::arch::asm!("mv {}, sp", out(reg) sp, options(nostack, nomem));
    }
    sp
}

/// Read the current page table root physical address from `satp`.
///
/// Extracts PPN from `satp[43:0]` and converts to a physical address.
///
/// # Safety
/// Must be called in S-mode.
#[cfg(not(test))]
pub unsafe fn read_root_phys() -> u64
{
    let satp: u64;
    // SAFETY: reading satp is safe in S-mode.
    unsafe {
        core::arch::asm!("csrr {}, satp", out(reg) satp, options(nostack, nomem));
    }
    // PPN is satp[43:0]; physical address = PPN << 12.
    (satp & 0x000F_FFFF_FFFF_FFFF) << 12
}

/// Map a single 4 KiB user page `virt` → `phys` in the Sv48 page table
/// rooted at `root_virt`, allocating missing intermediate frames from `allocator`.
///
/// Sets U (user) bit so userspace can access the mapping.
///
/// # Errors
/// Returns `Err(())` if the buddy allocator is exhausted.
///
/// # Safety
/// `root_virt` must be the direct-map virtual address of a valid 4 KiB Sv48
/// root frame. `virt` must be in the lower (user) half. `phys` must be 4 KiB-aligned.
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

    let root = unsafe { table_at(root_virt) };

    let l2_pa = rv_walk_or_alloc(&mut root[vpn3_index(virt)], allocator)?;
    let l2 = unsafe { table_at(phys_to_virt(l2_pa)) };

    let l1_pa = rv_walk_or_alloc(&mut l2[vpn2_index(virt)], allocator)?;
    let l1 = unsafe { table_at(phys_to_virt(l1_pa)) };

    let l0_pa = rv_walk_or_alloc(&mut l1[vpn1_index(virt)], allocator)?;
    let l0 = unsafe { table_at(phys_to_virt(l0_pa)) };

    // U bit (bit 4) allows user-mode access.
    const USER: u64 = 1 << 4;
    let mut pte = PageTableEntry::new_page(phys, flags);
    pte.0 |= USER;
    l0[vpn0_index(virt)] = pte;

    Ok(())
}

/// Walk an existing Sv48 page table entry or allocate a new child frame.
#[cfg(not(test))]
fn rv_walk_or_alloc(
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
    unsafe {
        core::ptr::write_bytes(frame_va as *mut u8, 0, PAGE_SIZE);
    }

    *entry = PageTableEntry::new_table(frame_pa);
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
    fn new_table_sets_valid_clears_rwx()
    {
        let pte = PageTableEntry::new_table(0x1000);
        assert!(pte.is_present());
        assert_eq!(pte.0 & READ, 0);
        assert_eq!(pte.0 & WRITE, 0);
        assert_eq!(pte.0 & EXECUTE, 0);
    }

    #[test]
    fn new_page_rw_sets_read_write_clears_execute()
    {
        let flags = PageFlags {
            readable: true,
            writable: true,
            executable: false,
        };
        let pte = PageTableEntry::new_page(0x2000, flags);
        assert!(pte.is_present());
        assert!(pte.0 & READ != 0);
        assert!(pte.0 & WRITE != 0);
        assert_eq!(pte.0 & EXECUTE, 0);
    }

    #[test]
    fn new_page_rx_sets_read_execute_clears_write()
    {
        let flags = PageFlags {
            readable: true,
            writable: false,
            executable: true,
        };
        let pte = PageTableEntry::new_page(0x3000, flags);
        assert!(pte.0 & READ != 0);
        assert_eq!(pte.0 & WRITE, 0);
        assert!(pte.0 & EXECUTE != 0);
    }

    #[test]
    fn phys_addr_roundtrip()
    {
        let pa: u64 = 0x8020_0000;
        let pte = PageTableEntry::new_table(pa);
        assert_eq!(pte.phys_addr(), pa);
    }

    #[test]
    fn is_present_false_for_zero_entry()
    {
        assert!(!PageTableEntry(0).is_present());
    }

    // ── VA index extraction ───────────────────────────────────────────────────

    #[test]
    fn direct_map_base_vpn3_index_is_256()
    {
        assert_eq!(vpn3_index(DIRECT_MAP_BASE), 256);
    }

    #[test]
    fn direct_map_base_lower_indices_are_zero()
    {
        assert_eq!(vpn2_index(DIRECT_MAP_BASE), 0);
        assert_eq!(vpn1_index(DIRECT_MAP_BASE), 0);
    }

    #[test]
    fn kernel_vbase_vpn3_is_511_vpn2_is_510()
    {
        let kv: u64 = 0xFFFF_FFFF_8000_0000;
        assert_eq!(vpn3_index(kv), 511);
        assert_eq!(vpn2_index(kv), 510);
        assert_eq!(vpn1_index(kv), 0);
    }
}
