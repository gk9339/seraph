/*
 * SPDX-License-Identifier: GPL-2.0-only
 * Copyright (C) 2026 George Kottler <mail@kottlerg.com>
 *
 * boot/loader/linker/riscv64-uefi.ld
 *
 * Linker script for the RISC-V UEFI bootloader. Produces an ELF that
 * llvm-objcopy converts to a flat PE32+ binary. The hand-crafted PE/COFF
 * header from header.S is placed at offset 0x000; the entry trampoline
 * and Rust code begin at 0x1000.
 */

OUTPUT_FORMAT("elf64-littleriscv")
OUTPUT_ARCH(riscv)

ENTRY(_start)

SECTIONS
{
    . = 0x0;

    /* PE/COFF header block, hand-assembled in header.S. */
    .pecoff_header : {
        KEEP(*(.pecoff_header))
    }

    /* Page-align to 0x1000 before the first section's data. */
    . = 0x1000;

    /* _start: RVA of the entry trampoline, placed in AddressOfEntryPoint. */
    _start = .;

    .text : {
        /* Entry trampoline must be first so it sits at AddressOfEntryPoint. */
        KEEP(*(.text.entry))
        *(.text .text.*)
    }

    .rodata : ALIGN(8) {
        *(.rodata .rodata.*)
    }

    .data : ALIGN(8) {
        *(.data .data.*)
    }

    .bss : ALIGN(8) {
        *(.bss .bss.*)
        *(COMMON)
    }

    /* _etext: end of all code and initialised data (used in PE SizeOfCode). */
    _etext = ALIGN(0x1000);

    /* Base-relocation table — required by EFI loaders even for PIC images. */
    .reloc : ALIGN(0x1000) {
        _reloc_start = .;
        KEEP(*(.reloc))
        _reloc_end = .;
    }

    /* _image_end: total image size in memory, aligned to SectionAlignment. */
    _image_end = ALIGN(0x1000);

    /DISCARD/ : {
        *(.eh_frame)
        *(.note.*)
        *(.comment)
        *(.riscv.attributes)
    }
}
