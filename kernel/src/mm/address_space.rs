// SPDX-License-Identifier: GPL-2.0-only
// Copyright (C) 2026 George Kottler <mail@kottlerg.com>

// kernel/src/mm/address_space.rs

//! User-mode address space management (Phase 9).
//!
//! An [`AddressSpace`] owns one root page table (PML4 on x86-64, Sv48 root
//! on RISC-V). Intermediate page table frames are allocated from the buddy
//! allocator on demand.
//!
//! ## Constants
//!
//! | Constant | Value | Meaning |
//! |---|---|---|
//! | `INIT_STACK_TOP` | `0x7FFF_FFFF_E000` | Virtual top of init's user stack |
//! | `INIT_STACK_PAGES` | 4 | Number of 4 KiB pages in the user stack (16 KiB) |
//!
//! ## Kernel mapping inheritance
//! `new_user` copies kernel PML4 entries [256..512] from the currently active
//! page table root into the new user PML4, so kernel memory is reachable from
//! user address spaces without per-process kernel mapping maintenance.
//!
//! On RISC-V the equivalent root entries are VPN[3] entries 256–511.
//!
//! ## Modification notes
//! - For SMP (Phase 10): TLB shootdown is needed on each `map_page` when
//!   other CPUs may have the same address space loaded.
//! - For W^X: `map_segment` already enforces no simultaneous W+X.

use boot_protocol::{InitSegment, SegmentFlags};

use crate::mm::{BuddyAllocator, PAGE_SIZE};
use crate::mm::paging::phys_to_virt;

/// Virtual address of the top of init's user stack.
///
/// `INIT_STACK_PAGES` pages are mapped immediately below this address.
/// One additional guard page (unmapped) sits below the stack.
pub const INIT_STACK_TOP: u64 = 0x7FFF_FFFF_E000;

/// Number of 4 KiB pages in init's user stack (16 KiB total).
pub const INIT_STACK_PAGES: usize = 4;

// ── AddressSpace ──────────────────────────────────────────────────────────────

/// A user-mode virtual address space.
///
/// Owns the physical frame of the root page table. All intermediate frames
/// allocated during mapping are tracked only implicitly through the page table
/// structure (full tracking + freeing is deferred to a future phase).
pub struct AddressSpace
{
    /// Physical address of the root page table frame (PML4 / Sv48 root).
    pub root_phys: u64,
    /// Virtual address of the root frame (via the direct physical map).
    pub root_virt: u64,
}

// SAFETY: AddressSpace is accessed only from the single boot thread in Phase 9.
unsafe impl Send for AddressSpace {}
unsafe impl Sync for AddressSpace {}

impl AddressSpace
{
    /// Allocate a new, empty user address space.
    ///
    /// 1. Allocates one frame from `allocator` for the root page table.
    /// 2. Zeros the frame.
    /// 3. Copies kernel-half entries (indices 256–511) from the current
    ///    hardware page table root so the kernel is reachable from this space.
    ///
    /// # Panics
    /// Calls `crate::fatal` if the buddy allocator is exhausted.
    ///
    /// # Safety
    /// Must be called after Phase 3 (page tables active) and Phase 4 (heap active).
    /// The current CPU's page table root must be the kernel's PML4/Sv48 root.
    #[cfg(not(test))]
    pub unsafe fn new_user(allocator: &mut BuddyAllocator) -> Self
    {
        // Allocate one 4 KiB frame (order 0) for the root page table.
        let root_phys = allocator
            .alloc(0)
            .unwrap_or_else(|| crate::fatal("address_space::new_user: out of memory for root PT"));

        let root_virt = phys_to_virt(root_phys);

        // Zero the frame (page table entries are 0 = not-present by default).
        // SAFETY: root_virt is a valid, exclusively-owned kernel virtual address.
        unsafe {
            core::ptr::write_bytes(root_virt as *mut u8, 0, PAGE_SIZE);
        }

        // Copy kernel-half PML4/Sv48 entries (indices 256–511) from the current
        // active page table root so the kernel stays accessible from user mode.
        //
        // On x86-64: read CR3 for the current PML4 physical address.
        // On RISC-V: read satp for the current Sv48 root physical address.
        unsafe {
            Self::copy_kernel_entries(root_virt);
        }

        Self {
            root_phys,
            root_virt,
        }
    }

    /// Copy entries 256–511 from the currently active page table root into
    /// the new user page table at `new_root_virt`.
    ///
    /// # Safety
    /// Both the current root and `new_root_virt` must be valid, 4 KiB-aligned
    /// kernel virtual addresses mapped R/W in the direct physical map.
    #[cfg(not(test))]
    unsafe fn copy_kernel_entries(new_root_virt: u64)
    {
        use crate::arch::current::paging::read_root_phys;

        let current_root_phys = unsafe { read_root_phys() };
        let current_root_virt = phys_to_virt(current_root_phys);

        // Each entry is 8 bytes; entries 256–511 start at byte offset 2048.
        let src = (current_root_virt + 2048) as *const u64;
        let dst = (new_root_virt + 2048) as *mut u64;

        // SAFETY: both src and dst are valid kernel virtual addresses within
        // 4 KiB page table frames; the 256 u64 copy (2048 bytes) stays within bounds.
        unsafe {
            core::ptr::copy_nonoverlapping(src, dst, 256);
        }
    }

    /// Map `virt` → `phys` as a 4 KiB page with the given permission flags.
    ///
    /// Allocates missing intermediate page table frames from `allocator`.
    ///
    /// # Safety
    /// `virt` must be in the user half (< 0x8000_0000_0000). `phys` must be
    /// a valid 4 KiB-aligned physical address.
    #[cfg(not(test))]
    pub unsafe fn map_page(
        &mut self,
        virt: u64,
        phys: u64,
        flags: crate::mm::paging::PageFlags,
        allocator: &mut BuddyAllocator,
    ) -> Result<(), ()>
    {
        use crate::arch::current::paging::map_user_page;

        // SAFETY: contract passed to caller.
        unsafe { map_user_page(self.root_virt, virt, phys, flags, allocator) }
    }

    /// Map each page of an ELF LOAD `segment` into this address space.
    ///
    /// Permissions are derived from `segment.flags`:
    /// - `Read`        → R-- (readable, not writable, not executable)
    /// - `ReadWrite`   → RW- (readable, writable; W^X: not executable)
    /// - `ReadExecute` → R-X (readable, executable; W^X: not writable)
    ///
    /// Physical addresses come from `segment.phys_addr`, mapped sequentially
    /// in 4 KiB increments across `segment.size` bytes (rounded up to pages).
    ///
    /// # Safety
    /// `segment` must be a valid, bootloader-provided InitSegment. `allocator`
    /// must be the kernel's buddy allocator.
    #[cfg(not(test))]
    pub unsafe fn map_segment(
        &mut self,
        segment: &InitSegment,
        allocator: &mut BuddyAllocator,
    ) -> Result<(), ()>
    {
        let flags = match segment.flags
        {
            SegmentFlags::Read =>
            {
                crate::mm::paging::PageFlags {
                    readable: true,
                    writable: false,
                    executable: false,
                }
            }
            SegmentFlags::ReadWrite =>
            {
                crate::mm::paging::PageFlags {
                    readable: true,
                    writable: true,
                    executable: false,
                }
            }
            SegmentFlags::ReadExecute =>
            {
                crate::mm::paging::PageFlags {
                    readable: true,
                    writable: false,
                    executable: true,
                }
            }
        };

        // Align virt and phys down to 4 KiB page boundaries for page table
        // mapping. The in-page offset is preserved implicitly: the CPU adds
        // (virt_addr & 0xFFF) to the physical frame address at access time.
        //
        // Example: virt_addr=0x201120 (off=0x120), phys_addr=0x1e1a6120
        //   → map virtual page 0x201000 → physical frame 0x1e1a6000
        //   → CPU translates 0x201120 → 0x1e1a6000 + 0x120 = 0x1e1a6120 ✓
        //
        // page_count includes the in-page offset so a segment that crosses a
        // page boundary gets enough pages mapped.
        let in_page_off = (segment.virt_addr & 0xFFF) as usize;
        let page_count = ((in_page_off + segment.size as usize + PAGE_SIZE - 1) / PAGE_SIZE).max(1);
        let virt_base = segment.virt_addr & !0xFFF_u64;  // page-aligned virtual
        let phys_base = segment.phys_addr & !0xFFF_u64;  // page-aligned physical frame
        for i in 0..page_count
        {
            let virt = virt_base + (i * PAGE_SIZE) as u64;
            let phys = phys_base + (i * PAGE_SIZE) as u64;
            // SAFETY: segment is bootloader-provided; caller's safety contract.
            unsafe {
                self.map_page(virt, phys, flags, allocator)?;
            }
        }
        Ok(())
    }

    /// Allocate `pages` physical frames and map them as a user stack.
    ///
    /// The stack occupies virtual addresses `[stack_top - pages * PAGE_SIZE, stack_top)`.
    /// One additional guard page (unmapped) sits below the stack to catch overflows.
    ///
    /// # Safety
    /// `stack_top` must be page-aligned and within the user address range.
    #[cfg(not(test))]
    pub unsafe fn map_stack(
        &mut self,
        stack_top: u64,
        pages: usize,
        allocator: &mut BuddyAllocator,
    ) -> Result<(), ()>
    {
        let rw_flags = crate::mm::paging::PageFlags {
            readable: true,
            writable: true,
            executable: false,
        };

        for i in 0..pages
        {
            // Allocate one physical frame per page.
            let phys = allocator
                .alloc(0)
                .ok_or(())?;

            // Zero the frame (stack pages should start clean).
            // SAFETY: phys_to_virt gives a valid kernel virtual address.
            unsafe {
                let virt = phys_to_virt(phys);
                core::ptr::write_bytes(virt as *mut u8, 0, PAGE_SIZE);
            }

            // Map at the correct virtual address (stack grows downward).
            let virt = stack_top - ((i + 1) * PAGE_SIZE) as u64;
            // SAFETY: phys is valid and virt is in user range.
            unsafe {
                self.map_page(virt, phys, rw_flags, allocator)?;
            }
        }

        // The guard page (one page below the stack) is intentionally left
        // unmapped: accessing it will fault, catching stack overflows.

        Ok(())
    }

    /// Activate this address space on the current CPU.
    ///
    /// On x86-64: writes `root_phys` to CR3 (flushes TLB).
    /// On RISC-V: writes the Sv48 SATP value and issues `sfence.vma`.
    ///
    /// # Safety
    /// Must be called at ring 0 / S-mode. After this call, all virtual
    /// addresses are resolved through this address space's page tables.
    #[cfg(not(test))]
    pub unsafe fn activate(&self)
    {
        use crate::arch::current::paging::activate;
        // SAFETY: caller's contract; root_phys is a valid page table root.
        unsafe { activate(self.root_phys); }
    }
}
