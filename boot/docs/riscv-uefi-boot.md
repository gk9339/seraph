# RISC-V UEFI Boot

## Overview

UEFI requires bootloader images to be PE32+ binaries. LLVM's x86-64 backend can emit
PE/COFF directly; its RISC-V backend cannot. The Seraph bootloader resolves this by
compiling the RISC-V bootloader as a position-independent ELF, prepending a
hand-crafted PE32+ header in assembly, and converting the result to a flat binary
with `llvm-objcopy`. The flat binary is a valid PE32+ image that UEFI firmware can
load and execute.

This document is self-contained and specific to RISC-V. x86-64 uses the standard
Rust PE/COFF output path and does not require this workaround.

---

## The Problem

UEFI firmware loads EFI applications as PE32+ images (the 64-bit variant of the
Windows PE format). On x86-64, the Rust compiler's built-in
`x86_64-unknown-uefi` target produces PE/COFF output directly — the linker emits a
`.efi` file that UEFI can parse and load without further processing.

LLVM's RISC-V backend has no PE/COFF output mode. Compiling the bootloader with a
RISC-V target produces an ELF file. UEFI firmware will not load an ELF file.

The solution used by the Linux kernel (in `arch/riscv/kernel/efi-header.S`) and
adopted here is:

1. Write a minimal PE32+ header in assembly that describes the image structure.
2. Place this header at the start of the output image.
3. Use a custom linker script to position the header before the Rust code.
4. Convert the ELF to a flat binary with `llvm-objcopy -O binary`, producing a
   file whose byte 0 is the DOS MZ signature that UEFI expects.

---

## PE/COFF Header Layout

The header is defined in `boot/loader/src/arch/riscv64/header.S`. All offsets are
relative to `pecoff_header_start`, which the linker places at address 0 in the final
binary (image base = 0 for an EFI application; UEFI relocates it to a free region).

```
Offset   Size   Content
──────   ────   ───────
0x000    64     DOS MZ stub
                  0x00: e_magic = 0x5A4D ('MZ')
                  0x3C: e_lfanew = offset of PE signature (0x40)
0x040     4     PE signature = "PE\0\0"
0x044    20     COFF file header
                  Machine          = 0x5064 (IMAGE_FILE_MACHINE_RISCV64)
                  NumberOfSections = 2 (.text, .reloc)
                  TimeDateStamp    = 0 (reproducible builds)
                  SizeOfOptionalHeader = sizeof(optional header)
                  Characteristics  = 0x020E
0x058   240     PE32+ optional header
                  Magic            = 0x020B (PE32+)
                  AddressOfEntryPoint = RVA of _start (= 0x1000)
                  BaseOfCode       = RVA of _start (= 0x1000)
                  ImageBase        = 0 (UEFI relocates)
                  SectionAlignment = 0x1000 (4 KiB)
                  FileAlignment    = 0x1000 (flat binary)
                  Subsystem        = 0x000A (EFI_APPLICATION)
                  NumberOfRvaAndSizes = 16
                  Data directories: only [5] (base relocation) is non-zero
0x148    40     .text section header
                  Name             = ".text\0\0\0"
                  VirtualAddress   = RVA of _start
                  SizeOfRawData    = _etext - _start
                  PointerToRawData = offset of _start in flat binary
                  Characteristics  = 0x60000020 (code, executable, readable)
0x170    40     .reloc section header
                  Name             = ".reloc\0\0"
                  VirtualAddress   = RVA of _reloc_start
                  SizeOfRawData    = 8 (one empty block, header only)
                  Characteristics  = 0x42000040 (initialised data, discardable, readable)
0x198   ~1640   Padding to 0x1000 (page boundary)
0x1000   —      .text section: entry trampoline (_start) followed by compiled Rust code
  ...    —      .reloc section: minimal base-relocation block (8 bytes)
```

The `Characteristics` field in the COFF file header is `0x020E`:
- `IMAGE_FILE_EXECUTABLE_IMAGE` (0x0002)
- `IMAGE_FILE_LINE_NUMS_STRIPPED` (0x0004)
- `IMAGE_FILE_LOCAL_SYMS_STRIPPED` (0x0008)
- `IMAGE_FILE_DEBUG_STRIPPED` (0x0200)

---

## Entry Trampoline

The entry trampoline is placed at `_start`, which is at `RVA 0x1000` — the
`AddressOfEntryPoint` in the PE32+ optional header.

```asm
    .balign 0x1000
    .section ".text.entry", "ax"
    .global _start
_start:
    // Tail-call into the Rust UEFI entry point.
    // UEFI calls this as: EFI_STATUS EFIAPI entry(EFI_HANDLE, EFI_SYSTEM_TABLE*)
    // RISC-V UEFI calling convention == lp64d ABI: a0=image_handle, a1=system_table.
    // efi_main in main.rs is declared `extern "efiapi"`, which on RISC-V is the
    // same as the C lp64d ABI. The tail call passes a0/a1 through unchanged.
    tail    efi_main
```

The `tail` pseudo-instruction expands to a PC-relative far jump using a temporary
register (`t1`). It does not save `ra`, making this a true tail call — control passes
to `efi_main` as if UEFI had called it directly.

---

## Minimal Base-Relocation Block

UEFI checks that the loaded image has a `.reloc` section before loading it. The
section must contain at least one base-relocation block. Since the bootloader is
compiled as a PIC ELF (no absolute addresses), there are no actual relocations to
apply; the block's relocation entry list is empty:

```asm
    .section ".reloc", "a"
    .long   0       // VirtualAddress: page 0 of image
    .long   8       // SizeOfBlock: 8 bytes = header only, zero relocation entries
```

An empty block (`SizeOfBlock = 8`) is valid per the PE/COFF specification —
it covers no addresses and applies no fixups. Its presence satisfies the UEFI loader's
requirement for a `.reloc` section without altering the image's runtime behaviour.

---

## Linker Script

`boot/loader/linker/riscv64-uefi.ld` controls the layout of the flat binary:

```
ENTRY(_start)

SECTIONS
{
    /* Header at offset 0 in the output binary. */
    .pecoff_header 0x0 : {
        KEEP(*(.pecoff_header))
    }

    /* .text.entry immediately follows, aligned to 0x1000.
       _start is the first symbol in this section. */
    .text 0x1000 : {
        _start = .;
        KEEP(*(.text.entry))
        *(.text .text.*)
        _etext = .;
    }

    /* Read-only data. */
    .rodata : {
        *(.rodata .rodata.*)
    }

    /* Writable data. */
    .data : {
        *(.data .data.*)
    }

    /* Zero-initialised data. */
    .bss : {
        *(.bss .bss.*)
        *(COMMON)
    }

    /* Base-relocation section. */
    .reloc : {
        _reloc_start = .;
        KEEP(*(.reloc))
        _reloc_end = .;
    }

    _image_end = .;

    /* Discard unnecessary ELF sections. */
    /DISCARD/ : {
        *(.eh_frame)
        *(.note.*)
        *(.comment)
        *(.gnu.*)
    }
}
```

The key symbols exported by the linker script for use in `header.S`:

| Symbol | Meaning |
|---|---|
| `_start` | First byte of the entry trampoline; also `AddressOfEntryPoint` RVA |
| `_etext` | End of the `.text` section; used to compute `SizeOfCode` |
| `_reloc_start` | Start of the `.reloc` section; base-relocation VirtualAddress |
| `_reloc_end` | End of the `.reloc` section |
| `_image_end` | End of the entire image; used for `SizeOfImage` |
| `pecoff_header_start` | Byte 0 of the image; all RVAs are relative to this symbol |

---

## Custom Target JSON

The RISC-V bootloader uses a custom Cargo target specification,
`scripts/targets/riscv64gc-seraph-uefi.json`. Key differences from the kernel target
(`riscv64gc-seraph-none.json`):

| Field | UEFI bootloader | Kernel |
|---|---|---|
| `os` | `"uefi"` | `"none"` |
| `llvm-target` | `"riscv64"` | `"riscv64"` |
| `relocation-model` | `"pic"` | `"static"` |
| `code-model` | `"medium"` | `"medium"` |
| `disable-redzone` | `true` | `true` |
| `features` | `"+m,+a,+f,+d,+c"` (RV64GC) | `"+m,+a,+f,+d,+c"` |
| `linker` | `"rust-lld"` | `"rust-lld"` |
| `linker-flavor` | `"ld.lld"` | `"ld.lld"` |
| `pre-link-args` | custom linker script | custom linker script |

The `relocation-model: "pic"` setting is critical — it instructs LLVM to emit PIC
code (PC-relative addressing, no absolute symbols). This is required because UEFI
loads the image at an arbitrary address chosen at runtime; the assembled flat binary
must be position-independent.

---

## Build Pipeline

The build process for the RISC-V UEFI image:

```
1. cargo build --target riscv64gc-seraph-uefi
   → Produces: target/riscv64gc-seraph-uefi/release/seraph-boot  (ELF)
   The ELF contains the .pecoff_header section (assembled from header.S) at
   load address 0x0, followed by .text at 0x1000.

2. llvm-objcopy -O binary \
       target/riscv64gc-seraph-uefi/release/seraph-boot \
       target/riscv64gc-seraph-uefi/release/seraph-boot.efi
   → Produces: seraph-boot.efi  (flat binary PE32+)
   llvm-objcopy strips all ELF structure and emits only the section data in
   order of load address. Byte 0 of the output is the MZ signature from
   .pecoff_header; byte 0x1000 is the entry trampoline.

3. Install seraph-boot.efi to \EFI\seraph\seraph-boot.efi on the ESP.
```

The ELF produced in step 1 is not a usable UEFI image — it has ELF headers that UEFI
does not understand. The flat binary from step 2 is the deliverable. Both files share
the same symbol table for debugging purposes; the ELF can be used with GDB or LLDB to
set symbolic breakpoints even though UEFI loads the flat binary.

---

## Maintenance Notes

### Obsolescence Path

If LLVM gains a RISC-V PE/COFF backend in a future release, the entire workaround
in this document becomes unnecessary. The RISC-V bootloader could use a standard
`riscv64-unknown-uefi` target (or equivalent Seraph custom target without `pic`
relocation model), and `header.S`, `riscv64-uefi.ld`, and this document could be
removed. Monitoring LLVM release notes for RISC-V PE/COFF backend support is
recommended.

### Validation Against UEFI Specification

The PE32+ header must conform to:
- UEFI Specification §2.1.1 — PE32+ image format requirements for EFI applications
- Microsoft PE/COFF Specification §3 (COFF file header), §4 (optional header),
  §5 (section table), §6 (base relocations)

Changes to the header structure (field values, section count, data directory entries)
must be validated against both specifications. In particular:
- `SizeOfImage` must be the exact byte size of the loaded image rounded up to
  `SectionAlignment`, not the flat file size
- `NumberOfRvaAndSizes` must match the number of data directory entries written
- The `.reloc` section's `VirtualAddress` in the data directory must match the
  section header's `VirtualAddress`

### Linux Kernel Reference

The design of `header.S` follows the approach in
`arch/riscv/kernel/efi-header.S` in the Linux kernel. That file documents the same
technique and has been validated against UEFI firmware implementations on production
RISC-V hardware. Divergences between the Linux kernel's header and the Seraph header
should be understood and documented; do not assume the Linux version is always correct
for Seraph's specific binary layout.
