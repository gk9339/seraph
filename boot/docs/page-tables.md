# Page Tables

## Overview

The bootloader establishes a minimal set of initial page tables before handing off to
the kernel. These tables serve one purpose: allow the kernel to begin executing at its
ELF virtual addresses and to access `BootInfo` and the boot modules before its own
page tables are ready. The kernel replaces them during Phase 3 of its initialisation
sequence; the bootloader's tables are temporary.

All page table frames are allocated via `AllocatePages` before `ExitBootServices`.
No page table allocation occurs after the firmware exits.

---

## What Gets Mapped

The initial page tables contain exactly three categories of mappings. Nothing else is
mapped; an access outside these ranges faults.

**Kernel ELF segments** — each LOAD segment is mapped at its ELF virtual address with
permissions derived from the ELF segment flags. This allows the kernel to execute from
the first instruction.

**Identity map of the boot region** — the `BootInfo` structure, the `PlatformResource`
array, the memory map buffer, and all boot modules are identity-mapped (virtual address
equals physical address). This allows the kernel to read them using physical addresses
before its direct physical map is established in Phase 3.

**Bootloader stack** — the stack in use at the point of kernel handoff is mapped at
its current virtual address. On x86-64 and RISC-V, the stack is allocated by UEFI
and its virtual address equals its physical address (UEFI runs with a 1:1 mapping
or a well-defined identity region). The stack mapping uses read-write, non-executable
permissions.

The UEFI firmware's own page tables (before `ExitBootServices`) already contain a
full 1:1 mapping of physical memory. After `ExitBootServices`, those page tables are
no longer in use; the bootloader installs its own minimal tables.

---

## Architecture Abstraction

Within the bootloader, page table construction is separated into an arch-neutral
interface and architecture-specific implementations:

```rust
/// Trait implemented by each architecture's page table builder.
pub trait PageTableBuilder: Sized
{
    /// Allocate a new, empty root page table. Frames come from UEFI AllocatePages.
    /// Returns None on allocation failure.
    fn new() -> Option<Self>;

    /// Map the virtual range [virt, virt+size) to [phys, phys+size) with the
    /// given flags. Size must be page-aligned. Returns Err on allocation failure
    /// or W^X violation.
    fn map(
        &mut self,
        virt: u64,
        phys: u64,
        size: u64,
        flags: PageFlags,
    ) -> Result<(), MapError>;

    /// Return the physical address of the root page table frame. This is the
    /// value written to CR3 (x86-64) or the PPN field of satp (RISC-V).
    fn root_physical(&self) -> u64;
}

#[derive(Debug)]
pub enum MapError
{
    /// Physical memory allocation for an intermediate table frame failed.
    OutOfMemory,
    /// The flags request both writable and executable permissions (W^X violation).
    WxViolation,
}

pub struct PageFlags
{
    pub readable:   bool,
    pub writable:   bool,
    pub executable: bool,
}
```

`PageFlags::writable && PageFlags::executable` is rejected by every `map`
implementation and returns `MapError::WxViolation`. This check is redundant with the
ELF loading check in [elf-loading.md](elf-loading.md), but both sites enforce W^X
independently to prevent a single failure mode from being missed.

Architecture-specific implementations live in `boot/loader/src/arch/x86_64/paging.rs`
and `boot/loader/src/arch/riscv64/paging.rs`. The `boot/loader/src/paging.rs` module
re-exports the active architecture's implementation.

---

## x86-64: 4-Level Paging

### Hierarchy

x86-64 with 4-level paging uses a four-level hierarchy indexed by bits of the virtual
address:

```
Virtual address bits:
  [47:39] → PML4 index (512 entries, 4 KiB table)
  [38:30] → PML3 (PDPT) index (512 entries, 4 KiB table)
  [29:21] → PML2 (PD) index (512 entries, 4 KiB table)
  [20:12] → PML1 (PT) index (512 entries, 4 KiB table)
  [11:0]  → Byte offset within the 4 KiB page
```

The root table (PML4) occupies one 4 KiB frame. Each entry is a 64-bit value. Present
entries in PML4 and PML3 point to the next-level table's physical frame. PML1 entries
(PTEs) point to the final 4 KiB data frame.

### PTE Format

```
Bit 0    (P):   Present
Bit 1    (R/W): 1 = Writable; 0 = Read-only
Bit 2    (U/S): 0 = Supervisor only (all bootloader mappings are supervisor-only)
Bit 3    (PWT): 0 (write-back caching; no special caching for kernel mappings)
Bit 4    (PCD): 0
Bit 5    (A):   Accessed (set by hardware; initialised to 0)
Bit 6    (D):   Dirty (PTE only; initialised to 0)
Bit 12–51:      Physical frame number (PFN, physical address >> 12)
Bit 63   (NX):  1 = No-execute (set for all non-executable mappings)
```

Permission mapping:

| `PageFlags` | R/W bit | NX bit |
|---|---|---|
| Readable only | 0 (read-only) | 1 (NX) |
| Readable + Writable | 1 | 1 (NX) |
| Readable + Executable | 0 (read-only) | 0 (executable) |

W^X: the combination Writable=1 and NX=0 is never written; `map` returns
`MapError::WxViolation` before any table is modified.

### Intermediate Table Allocation

Each new PML3, PML2, or PML1 table requires one 4 KiB frame. Frames are allocated
via `AllocatePages(AllocateAnyPages, EfiLoaderData, 1, &addr)` and zeroed before use.
Zeroing ensures absent entries have `P=0` and the NX bit set, preventing accidental
execution of zeroed memory on non-NX-aware processors (though SMEP would catch this
anyway on modern hardware).

### Activation

```rust
// SAFETY: root_phys is the physical address of a valid, complete PML4 table.
// Interrupts are disabled. After this instruction, virtual addresses are
// interpreted according to the new table; all required mappings are present.
unsafe
{
    core::arch::asm!("mov cr3, {0}", in(reg) root_phys, options(nostack));
}
```

Writing CR3 flushes all TLB entries that are not tagged as global. The bootloader's
tables do not use the Global bit (`G=0` in all PTEs) because TLB flushing is correct
and the tables are short-lived.

---

## RISC-V: Sv48 Paging

### Hierarchy

RISC-V with Sv48 uses a four-level hierarchy (root, level-2, level-1, level-0):

```
Virtual address bits (Sv48):
  [47:39] → Root table index (512 entries)
  [38:30] → Level-2 table index (512 entries)
  [29:21] → Level-1 table index (512 entries)
  [20:12] → Level-0 table index (512 entries)
  [11:0]  → Byte offset within the 4 KiB page
```

Each table is 4 KiB and holds 512 eight-byte PTEs. The root table physical address
is right-shifted by 12 bits to produce the PPN for the `satp` register.

### PTE Format (RISC-V Sv48)

```
Bit 0    (V):   Valid
Bit 1    (R):   Readable
Bit 2    (W):   Writable
Bit 3    (X):   Executable
Bit 4    (U):   User-accessible (0 for all bootloader mappings — S-mode only)
Bit 5    (G):   Global (0; not used by the bootloader)
Bit 6    (A):   Accessed (initialised to 1 to avoid access-flag faults on hardware
                that does not set A/D bits in hardware and would fault instead)
Bit 7    (D):   Dirty (initialised to 1 for writable pages; same rationale as A)
Bits 10:8 (RSW): Reserved for software; set to 0
Bits 53:10 (PPN): Physical page number (physical address >> 12)
Bits 63:54: Reserved; must be 0
```

A PTE is a leaf if R=1 or X=1 (or both). A PTE is a pointer to the next-level table
if R=0, W=0, X=0, and V=1.

Permission mapping:

| `PageFlags` | R | W | X |
|---|---|---|---|
| Readable only | 1 | 0 | 0 |
| Readable + Writable | 1 | 1 | 0 |
| Readable + Executable | 1 | 0 | 1 |

W^X: W=1 and X=1 is rejected by `map` before any table is modified.

### Intermediate Table Allocation

Intermediate table frames are allocated and zeroed identically to x86-64. A zeroed
PTE has V=0 and is invalid, which is the correct initial state.

### Activation

```rust
// Construct satp: MODE=9 (Sv48), ASID=0, PPN=root_phys>>12
let satp = (9u64 << 60) | (root_phys >> 12);
// SAFETY: satp encodes a valid Sv48 root table at root_phys. All mappings
// required for continued execution are present. SFENCE.VMA flushes stale
// TLB entries before the new translation takes effect.
unsafe
{
    core::arch::asm!(
        "csrw satp, {satp}",
        "sfence.vma",
        satp = in(reg) satp,
        options(nostack),
    );
}
```

ASID 0 is used for the bootloader's tables. The kernel uses ASID 0 for its own
initial context (per the boot protocol's description of kernel entry state) and
reassigns ASIDs when it brings up its own page table management in Phase 3.

---

## W^X Enforcement

W^X is checked at two levels:

1. **ELF loading** ([elf-loading.md](elf-loading.md)) — any segment with `PF_W | PF_X`
   is fatal before any frame is allocated.
2. **Page table mapping** — the `map` function rejects `PageFlags { writable: true,
   executable: true }` with `MapError::WxViolation`.

Both checks are present because ELF loading and page table construction are separate
steps, and a violation at either point is equally dangerous. A writable+executable
mapping that reaches the kernel is a security defect, not just a policy violation.

---

## Page Table Frame Tracking

The bootloader does not free page table frames. All intermediate table frames
allocated by `AllocatePages` appear in the UEFI memory map as `EfiLoaderData`
regions, which translate to `MemoryKind::Loaded` in `BootInfo`. The kernel sees
these regions as in-use and does not reclaim them until it establishes its own page
tables in Phase 3, after which it can safely free the bootloader's intermediate
frames via its buddy allocator.

The bootloader records the root page table's physical address but does not
separately track intermediate frames. The kernel does not need to enumerate them
— it simply replaces the entire page table structure during Phase 3 and the old
frames become reclaimable as `EfiLoaderData` entries in the memory map are processed.
