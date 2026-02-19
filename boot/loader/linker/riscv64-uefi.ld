/* boot/loader/linker/riscv64-uefi.ld
 *
 * Linker script for the RISC-V UEFI bootloader flat binary.
 *
 * The resulting ELF is converted to a flat binary by:
 *   llvm-objcopy -O binary seraph-boot BOOTRISCV64.EFI
 *
 * UEFI firmware then loads BOOTRISCV64.EFI as a PE32+ application, using the
 * hand-crafted PE/COFF header placed at offset 0 by header.S.
 *
 * Section layout (VMAs = file offsets in the flat binary):
 *   0x000000  .pecoff_header   PE/COFF header block (from header.S)
 *   0x001000  .text            Entry trampoline + Rust code
 *             .rodata          Read-only data
 *             .data            Initialised data
 *             .bss             Zero-initialised data
 *   aligned   .reloc           Minimal base-relocation block (from header.S)
 *
 * All symbols referenced in header.S (_start, _etext, _reloc_start,
 * _reloc_end, _image_end) are defined here.
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
