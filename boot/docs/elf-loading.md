# ELF Loading

## Overview

The bootloader loads two categories of ELF binaries from the EFI System Partition:
the kernel ELF and one or more boot modules. The kernel ELF is loaded with full
validation and segment placement; boot modules are loaded as opaque flat binaries.
Both are placed in physical memory allocated via UEFI before `ExitBootServices`.

W^X is enforced at load time. Any ELF segment with both write and execute permissions
is a fatal error — the bootloader will not establish a mapping it cannot make safe.

---

## File Paths

Files are opened via `EFI_SIMPLE_FILE_SYSTEM_PROTOCOL` on the ESP volume:

| File | Path on ESP |
|---|---|
| Kernel | `\EFI\seraph\seraph-kernel` |
| Init binary (first module) | `\EFI\seraph\init` |

All paths use backslash separators as required by the UEFI file protocol. Both files
must be present; their absence is a fatal error. Additional modules (beyond init) are
an extension point for future use; their paths would be listed in a configuration
file or hard-coded by convention.

---

## ELF Validation

The kernel ELF is validated before any segment is loaded. All validation failures are
fatal. The checks, in order:

```
1. File is at least sizeof(Elf64_Ehdr) bytes.
2. e_ident[0..4] == [0x7F, b'E', b'L', b'F'] — ELF magic.
3. e_ident[EI_CLASS] == ELFCLASS64 — 64-bit ELF.
4. e_ident[EI_DATA] == ELFDATA2LSB — little-endian (both target architectures).
5. e_ident[EI_VERSION] == EV_CURRENT — ELF version 1.
6. e_type == ET_EXEC — static executable; position-independent executables
   (ET_DYN) are not supported for the kernel.
7. e_machine:
   - x86-64: EM_X86_64 (0x3E)
   - RISC-V: EM_RISCV (0xF3)
   Mismatch between the ELF machine type and the bootloader's build architecture
   is a fatal error.
8. e_phentsize == sizeof(Elf64_Phdr) — program header entry size matches expected.
9. e_phnum > 0 — at least one program header.
10. e_entry is within the address range of at least one LOAD segment.
```

Validation of `e_entry` (check 10) requires reading the program headers first; this
check is performed after the program headers are read.

Boot modules (including init) are not ELF-validated by the bootloader. They are
treated as opaque byte sequences and loaded into contiguous physical memory. The
kernel ELF-validates each module in Phase 9 of its initialisation sequence before
execution.

---

## LOAD Segment Processing

After ELF validation, all `PT_LOAD` program header entries are processed in order.
For each LOAD segment:

```
1. Read p_paddr (physical address), p_filesz (file size), p_memsz (memory size),
   p_offset (file offset), p_flags (permissions).
2. Validate:
   a. p_memsz >= p_filesz (memory size must accommodate the file content plus BSS).
   b. p_align is a power of two and >= PAGE_SIZE, or p_align == 0.
   c. (p_flags & PF_W) && (p_flags & PF_X) is false — W^X enforcement.
      A segment with both PF_W and PF_X is a fatal error.
3. Allocate physical frames:
   - Call AllocatePages(AllocateAddress, EfiLoaderData, page_count, &p_paddr).
   - page_count covers the range [p_paddr, p_paddr + p_memsz) rounded to pages.
4. Read p_filesz bytes from file offset p_offset into the allocated region.
5. Zero the BSS tail: memset(phys + p_filesz, 0, p_memsz - p_filesz).
6. Record the segment's physical address, virtual address (p_vaddr), size, and
   permission flags for use in page table construction.
```

Permissions are recorded per segment because the page table builder
([page-tables.md](page-tables.md)) needs the exact `(readable, writable, executable)`
flags to set page table entries correctly. The typical kernel ELF segments:

| Segment | Permissions | Page table flags |
|---|---|---|
| `.text` | `PF_R | PF_X` | Readable, Executable |
| `.rodata` | `PF_R` | Readable only |
| `.data`, `.bss` | `PF_R | PF_W` | Readable, Writable |

The `.bss` segment has `p_filesz == 0` and `p_memsz > 0`; the entire allocation is
zeroed in step 5. No file read occurs for a pure BSS segment.

---

## Entry Point Extraction

The kernel entry point is `e_entry` from the ELF header. This is a virtual address.
The corresponding physical address (needed for the initial jump before paging is
active) is computed by finding the LOAD segment whose virtual range contains `e_entry`
and applying the `p_vaddr → p_paddr` offset for that segment:

```
physical_entry = e_entry - segment.p_vaddr + segment.p_paddr
```

Both the virtual and physical entry point addresses are recorded. The bootloader jumps
to the physical address if paging is not yet enabled; it jumps to the virtual address
after page tables are installed.

In practice, the page tables are installed in the bootloader before the jump
([page-tables.md](page-tables.md)), so the jump target is the ELF virtual address.

---

## Boot Module Loading

Boot modules are loaded as flat binary regions. The init binary is always loaded
first and occupies `modules.entries[0]`. Module loading:

```
1. Open the module file and query its size via EFI_FILE_INFO.
2. AllocatePages(AllocateAnyPages, EfiLoaderData, page_count, &phys_base).
   page_count = ceil(file_size / PAGE_SIZE).
3. Read the entire file into the allocated region.
4. Pad the region to a page boundary if the file size is not page-aligned
   (the extra bytes are zero from AllocatePages, which zeroes pages on allocation).
5. Record phys_base and file_size in a BootModule entry.
```

The physical address chosen by `AllocateAnyPages` is recorded as `BootModule.physical_base`.
The file size (not the rounded allocation size) is recorded as `BootModule.size`, so
the kernel knows the exact extent of valid data. The kernel ELF-validates each module
before use.

---

## Extensibility

The current design hard-codes the kernel path and init module path. Future extension
points:

- A boot configuration file on the ESP (`\EFI\seraph\boot.cfg`) could specify
  additional module paths and a kernel command line, replacing hard-coded paths.
- The `BootInfo.modules` array can accommodate any number of modules; the kernel
  iterates `modules.count` without assuming a fixed count.
- Additional modules would follow init in the array, with their purpose established
  by convention between the kernel and the service using them.

These extensions do not require protocol changes as long as the first module remains
init and `BootInfo.modules.count` is accurate.
