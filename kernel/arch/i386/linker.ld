/* The bootloader will look at this image and start execution at the symbol
 * designated at the entry point. */
ENTRY(_start)

/* Tell where the various sections of the object files will be put in the final
 * kernel image. */
SECTIONS
{
    /* The kernel will live at 3GB + 1MB in the virtual address space,
     * which will be mapped to 1MB in the physical address space. */
    . = 1M;
    phys = .;

    /* Add a symbol that indicates the start of the kernel */
    _kernel_start = .;
	.text BLOCK(4K) : ALIGN (4K)
	{
        code = .;
		*(.multiboot)
		*(.text)
	}
	
    /* Read-only data. */
	.rodata BLOCK(4K) : ALIGN (4K)
	{
		*(.rodata)
	}

	/* Read-write data (initialized) */
	.data BLOCK(4K) : ALIGN (4K)
	{
        data = .;
		*(.data)
        *(.symbols)
        PROVIDE(kernel_symbols_start = .);
        PROVIDE(kernel_symbols_end = .);
	}
	
    /* Read-write data (uninitialized) and stack */
	.bss BLOCK(4K) : ALIGN (4K)
	{
        bss = .;
		*(COMMON)
		*(.bss)
		*(.kernel_stack)
	}
	/* Add a symbol that indicates the end address of the kernel. */
	_kernel_end = .;
	
    /DISCARD/ :
	{
		*(.comment)
		*(.note.gnu.build-id)
    }
}
