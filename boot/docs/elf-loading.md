# ELF Loading

The bootloader loads three categories of binaries from the EFI System Partition:

- **Kernel ELF** — fully validated and loaded at ELF-specified physical addresses.
- **Init ELF** — fully validated and ELF-parsed; segments allocated at any available
  physical address. Result is an `InitImage` passed to the kernel in `BootInfo.init_image`.
- **Boot modules** — opaque flat binaries loaded verbatim into physical memory and
  passed to the kernel via `BootInfo.modules`. The bootloader does not inspect or
  interpret their content; what they are is init's concern.

All loading occurs before `ExitBootServices`. W^X is enforced for the kernel and
init ELFs at load time: any `PT_LOAD` segment with both write and execute permissions
is a fatal error.

---

## File Paths

Files are opened via `EFI_SIMPLE_FILE_SYSTEM_PROTOCOL` on the ESP volume. Paths
come from `\EFI\seraph\boot.conf`, parsed before any file loading occurs (see
[uefi-environment.md](uefi-environment.md)):

| File | Config key | Default path on ESP |
|---|---|---|
| Kernel | `kernel` | `\EFI\seraph\seraph-kernel` |
| Init binary | `init` | `\EFI\seraph\init` |
| Boot modules | future `boot.conf` keys | — |

All paths use backslash separators as required by the UEFI file protocol. The kernel
and init keys are required; their absence is a fatal error. Additional module paths
are an extension point via new keys in `boot.conf`; the parser silently skips
unknown keys, so old bootloader binaries are unaffected by additions.

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

The same ten checks apply to the init ELF. Boot modules (the `BootInfo.modules`
slice) are not ELF-validated by the bootloader — they are loaded as opaque flat
binaries. Their validation and execution is init's responsibility.

---

## LOAD Segment Processing

After ELF validation, all `PT_LOAD` program header entries are processed in order.
For each LOAD segment:

```
1. Read p_paddr (physical address), p_filesz (file size), p_memsz (memory size),
   p_offset (file offset), p_flags (permissions).
2. Validate:
   a. p_memsz >= p_filesz (memory size must accommodate the file content plus BSS).
   b. p_align validation (power-of-two, >= PAGE_SIZE) — **not yet implemented**.
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

## Init ELF Loading

Init is loaded and pre-parsed into an `InitImage` for the kernel. The procedure
differs from kernel loading in one key respect: init is a userspace ELF whose
`p_paddr` values are in low memory already occupied by UEFI firmware, so segments
are allocated at any available physical address via `AllocateAnyPages` rather than
`AllocateAddress`.

```
For each PT_LOAD segment:
1. AllocatePages(AllocateAnyPages, EfiLoaderData, page_count, &phys_base).
   page_count = ceil(p_memsz / PAGE_SIZE).
2. Copy p_filesz bytes from file offset p_offset into phys_base.
3. Zero the BSS tail: memset(phys_base + p_filesz, 0, p_memsz - p_filesz).
4. Record an InitSegment { phys_addr, virt_addr: p_vaddr, size: p_memsz, flags }.
```

`flags` is derived from `p_flags`: `ReadExecute` if `PF_X` is set, `ReadWrite` if
`PF_W` is set (and `PF_X` is not), otherwise `Read`. The resulting `InitImage`
(entry point + segment array) is stored in `BootInfo.init_image`. The kernel uses
the `phys_addr`/`virt_addr` pairs to build init's page tables without an ELF parser.

---

## Boot Module Loading

Boot modules are flat binary images for early userspace services (e.g. procmgr,
devmgr). The bootloader loads whatever files `boot.conf` specifies; it does not
interpret their purpose.

```
1. Open the module file and query its size via EFI_FILE_INFO.
2. AllocatePages(AllocateAnyPages, EfiLoaderData, page_count, &phys_base).
   page_count = ceil(file_size / PAGE_SIZE).
3. Read the entire file into the allocated region.
4. The allocated region may be larger than the file if the file size is not
   page-aligned; the extra bytes at the end are unused (not explicitly zeroed).
5. Record phys_base and file_size in a BootModule entry in BootInfo.modules.
```

`BootModule.size` records the exact file size (not the rounded allocation size).
Init receives the module slice via its initial CSpace and is responsible for
validating and starting each service.

---

## Extensibility

All file paths come from `\EFI\seraph\boot.conf`, not hard-coded in the bootloader
binary. Adding boot modules requires only new keys in `boot.conf`; the parser
silently skips unknown keys, so existing bootloader binaries are unaffected.
`BootInfo.modules.count` accurately reflects however many modules were loaded; the
kernel and init iterate it without assuming a fixed count or fixed ordering.

---

## Summarized By

None
