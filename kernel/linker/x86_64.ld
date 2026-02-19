/*
 * SPDX-License-Identifier: GPL-2.0-only
 * Copyright (C) 2026 George Kottler <mail@kottlerg.com>
 *
 * kernel/linker/x86_64.ld
 *
 * Linker script for the Seraph kernel (x86-64). Places the kernel image
 * at 0xFFFFFFFF80000000; the physical direct map is established at runtime
 * at 0xFFFF800000000000.
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
