# Boot Flow

## Overview

This document describes the bootloader's execution from `efi_main` to the kernel
handoff. It is the orchestration document for the `boot/loader/` crate — each
numbered step below has a dedicated document covering its implementation. The
contract that the bootloader must satisfy at the point of handoff is defined in
[docs/boot-protocol.md](../../docs/boot-protocol.md); read that document first.

This document covers *how* the bootloader fulfils that contract, not what the
contract requires.

---

## Boot Sequence

The following ten steps correspond to the bootloader's execution order. Each step is
described briefly here; detailed implementation is in the referenced document.

### Step 1: UEFI Protocol Discovery

`efi_main` receives an `EFI_HANDLE image_handle` and a pointer to the UEFI system
table. The first act is to locate the protocols needed for the rest of the boot:

- `EFI_LOADED_IMAGE_PROTOCOL` — to find the device handle for the boot volume
- `EFI_SIMPLE_FILE_SYSTEM_PROTOCOL` — to open the EFI System Partition filesystem
- `EFI_GRAPHICS_OUTPUT_PROTOCOL` — to record the framebuffer, if present

Protocol handles are resolved via `BootServices->HandleProtocol` and
`BootServices->LocateProtocol`. Failure to locate a required protocol is fatal.

Detail: [uefi-environment.md](uefi-environment.md)

### Step 2: Load Boot Configuration

The bootloader opens `\EFI\seraph\boot.conf` on the ESP and reads it into a
4096-byte stack buffer. The file is parsed line by line for `key=value` entries.
`#` comments and blank lines are ignored; unknown keys are skipped for forward
compatibility. The required keys are `kernel` and `init`; a missing key or
malformed line is a fatal error (`InvalidConfig`).

The parsed paths are used in all subsequent file-open operations, replacing any
hardcoded path strings in the bootloader binary.

Detail: [uefi-environment.md](uefi-environment.md)

### Step 3: Load Kernel ELF

The kernel ELF is loaded from the path specified by the `kernel` key in
`boot.conf` (default: `\EFI\seraph\seraph-kernel`). The ELF header is validated,
LOAD segments are mapped into physical memory allocated via `AllocatePages`, and
the kernel virtual addresses and entry point are recorded.

W^X is enforced during loading: any ELF segment requesting both writable and
executable permissions is a fatal error.

Detail: [elf-loading.md](elf-loading.md)

### Step 4: Load Boot Modules

Boot modules are loaded from the ESP. The init binary path comes from the `init`
key in `boot.conf` (default: `\sbin\init`) and is always the first module
(`modules.entries[0]`). Each module is loaded as a contiguous physical allocation.
The base address and size of each module are recorded for inclusion in `BootInfo`.

Detail: [elf-loading.md](elf-loading.md)

### Step 5: Firmware Discovery

The UEFI configuration table is scanned for two GUIDs:
- `EFI_ACPI_20_TABLE_GUID` → physical address of the ACPI RSDP
- `EFI_DTB_TABLE_GUID` → physical address of the Device Tree blob

Both GUIDs are searched unconditionally; absent entries produce a zero field in
`BootInfo`. No ACPI or Device Tree content is parsed here — that is deferred to
`devmgr` in userspace. The `platform_resources` slice in `BootInfo` is empty at
this stage (count = 0).

Detail: [firmware-parsing.md](firmware-parsing.md)

### Step 6: Allocate and Build Page Tables

Initial page tables are constructed for the kernel. All page table frames are
allocated from UEFI before `ExitBootServices`. The tables map:

- The kernel ELF segments at their ELF virtual addresses, with segment permissions
- An identity map of the `BootInfo` structure, all boot modules, and the bootloader's
  own stack, so the kernel can read them before replacing the page tables

W^X is verified during construction: no PTE has both writable and executable bits.

Detail: [page-tables.md](page-tables.md)

### Step 7: Query Final Memory Map

The UEFI memory map is queried immediately before `ExitBootServices`. Every UEFI
allocation performed after the previous query invalidates the map key; this final
query must be the last allocation-generating action before the exit call. The map is
translated from UEFI memory types to the `MemoryType` values defined in the boot
protocol and sorted by `physical_base`.

Detail: [uefi-environment.md](uefi-environment.md)

### Step 8: ExitBootServices

`ExitBootServices` is called with the map key from step 7. If the call fails due to
a stale key (indicating that UEFI performed allocations between the query and the
call), the memory map is re-queried and the call is retried once. After a successful
exit, UEFI boot services are unavailable; no further UEFI calls are made.

Detail: [uefi-environment.md](uefi-environment.md)

### Step 9: Populate BootInfo

`BootInfo` is populated in-place in a physical memory region allocated before step 8.
All pointer and address fields hold physical addresses; no virtual addresses appear in
`BootInfo`. The `version` field is set to `BOOT_PROTOCOL_VERSION` (currently `2`).
Fields are populated as follows:

| Field | Source |
|---|---|
| `version` | `BOOT_PROTOCOL_VERSION` constant from `boot-protocol` crate |
| `memory_map` | Translated UEFI memory map from step 7 |
| `kernel_physical_base` | Physical address of kernel LOAD segments from step 3 |
| `kernel_virtual_base` | ELF virtual base address from step 3 |
| `kernel_size` | Total span of kernel ELF LOAD segments from step 3 |
| `modules` | Physical base and size of each boot module from step 4 |
| `framebuffer` | GOP framebuffer from step 1 (zeroed if GOP is absent) |
| `acpi_rsdp` | Physical address of ACPI RSDP from step 5; zero if GUID absent |
| `device_tree` | Physical address of DTB from step 5; zero if GUID absent |
| `platform_resources` | Empty (count = 0); `PlatformResource` extraction deferred to a future milestone |
| `command_line` | Physical address of null-terminated ASCII string; may be empty |

All arrays pointed to by `BootInfo` fields reside in physical memory that the UEFI
memory map marks as `Loaded` or `Usable`, ensuring they survive until the kernel
reclaims or remaps them.

### Step 10: Kernel Handoff

CPU state is established per the boot protocol and the kernel entry point is called.
This step is the point of no return: the bootloader has no code to execute after the
jump, and `kernel_entry` is declared `-> !`.

See [Kernel Handoff](#kernel-handoff) for the architecture-specific setup.

---

## BootInfo Population Details

Every pointer in `BootInfo` is a physical address. The kernel cannot dereference
these pointers through its own virtual address space until its direct physical map is
active (Phase 3 of kernel initialisation). Before that point, the kernel accesses
`BootInfo` fields through the identity mapping established in step 5.

The `BootInfo` structure itself must not be placed in a region the kernel will
reclaim before reading all fields. In practice this means placing it in a range the
memory map marks as `Loaded`, which the kernel treats as in-use until it explicitly
chooses to reclaim it.

Slices within `BootInfo` (`memory_map`, `modules`, `platform_resources`) point to
separately allocated physical regions. These regions must also remain readable until
the kernel has consumed them.

---

## Kernel Handoff

### x86-64

```
1. Clear DF (direction flag) via CLD
2. CLI — interrupts remain disabled (they are already disabled post-ExitBootServices,
   but this is explicit)
3. Install the page table: MOV cr3, <root PML4 physical address>
   (a full TLB flush occurs because the previous CR3 is replaced)
4. Set rdi = physical address of BootInfo (first argument register, System V AMD64 ABI)
5. JMP to kernel_entry — does not return
```

The bootloader-provided GDT remains active. The kernel replaces it in Phase 5 of
its initialisation sequence. The IDT is not loaded; interrupts must stay disabled
until the kernel installs its own. SSE/AVX are not initialised.

### RISC-V (RV64GC)

```
1. Ensure sstatus.SIE = 0 (interrupts disabled)
2. Install the page table: write Sv48 satp value (MODE=9, ASID=0, PPN=root/4096)
   via CSRW satp; execute SFENCE.VMA to flush the TLB
3. Set a0 = physical address of BootInfo (first argument register)
4. Set a1 = hart ID obtained from EFI_RISCV_BOOT_PROTOCOL during step 1
5. JALR to kernel_entry — does not return
```

Secondary harts remain in the UEFI firmware's spin loop or halted state. The kernel
releases them in Phase 10 of its initialisation sequence via SBI HSM calls.

---

## Shared Types: boot-protocol Crate

The `boot-protocol` crate (`boot/protocol/`) defines all types shared between the
bootloader and the kernel. Its constraints:

- **`#![no_std]`** — no standard library dependency; the crate links into both the
  UEFI bootloader and the `no_std` kernel without modification
- **`#[repr(C)]`** on all shared types — layout must be stable across independently
  compiled crates and future compiler versions
- **`BOOT_PROTOCOL_VERSION: u32 = 2`** — a version constant embedded by both the
  bootloader and the kernel; the kernel halts at entry if the field value does not
  match this constant

The crate contains no logic — only type definitions and the version constant. It must
not import any crate that has an `std` dependency. When the boot protocol changes in
an incompatible way (new fields, reordered fields, changed enum discriminants),
`BOOT_PROTOCOL_VERSION` is incremented and both the bootloader and the kernel are
updated in the same commit.
