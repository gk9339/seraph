/*
 * x86_64.ld -- Linker script for the Seraph kernel (x86-64).
 *
 * The kernel code model places all kernel code and static data within the
 * uppermost 2 GiB of the virtual address space (above 0xFFFFFFFF80000000).
 * The direct physical map occupies the region starting at PHYSMAP_BASE
 * (0xFFFF800000000000), leaving the upper 2 GiB for the kernel image.
 *
 * Memory layout (virtual):
 *   0xFFFFFFFF80000000  kernel image base (text, rodata, data, bss)
 *   0xFFFF800000000000  physical memory direct map (set up at runtime)
 *
 * The bootloader loads the kernel ELF and establishes page table mappings
 * at the addresses specified here before jumping to kernel_entry.
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
