/*
 * riscv64.ld -- Linker script for the Seraph kernel (RISC-V RV64GC).
 *
 * The kernel uses the medium (medany) code model, which generates PC-relative
 * addresses within a ±2 GiB range. The kernel image is placed in the upper
 * half of the Sv48 address space (above 0xFFFFFFFF80000000), matching the
 * x86-64 layout so that memory-model reasoning applies uniformly.
 *
 * The direct physical map occupies the region starting at PHYSMAP_BASE
 * (0xFFFF800000000000), established at runtime by the kernel.
 *
 * Memory layout (virtual):
 *   0xFFFFFFFF80000000  kernel image base (text, rodata, data, bss)
 *   0xFFFF800000000000  physical memory direct map (set up at runtime)
 */

KERNEL_BASE = 0xFFFFFFFF80000000;

ENTRY(kernel_entry)

SECTIONS
{
    . = KERNEL_BASE;

    .text ALIGN(4K) :
    {
        *(.text .text.*)
    }

    .rodata ALIGN(4K) :
    {
        *(.rodata .rodata.*)
    }

    .data ALIGN(4K) :
    {
        *(.data .data.*)
    }

    .bss ALIGN(4K) :
    {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    }

    /DISCARD/ :
    {
        *(.comment)
        *(.note .note.*)
        *(.eh_frame .eh_frame_hdr)
    }
}
