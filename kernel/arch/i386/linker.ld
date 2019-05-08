/* The bootloader will look at this image and start execution at the symbol
 * designated at the entry point. */
ENTRY(_start)

/* Tell where the various sections of the object files will be put in the final
 * kernel image. */
SECTIONS
{
    /* The kernel will live at 3GB + 1MB in the virtual address space,
     * which will be mapped to 1MB in the physical address space. */
    . = 0xC0100000;
    
    /* Add a symbol that indicates the start of the kernel */
    _kernel_start = .;
	.text ALIGN (4K) : AT (ADDR (.text) - 0xC0000000)
	{
		*(.multiboot)
		*(.text)
	}
	
    /* Read-only data. */
	.rodata ALIGN (4K) : AT (ADDR (.rodata) - 0xC0000000)
	{
		*(.rodata)
	}

	/* Read-write data (initialized) */
	.data ALIGN (4K) : AT (ADDR (.data) - 0xC0000000)
	{
		*(.data)
	}
	
    /* Read-write data (uninitialized) and stack */
	.bss ALIGN (4K) : AT (ADDR (.bss) - 0xC0000000)
	{
		*(COMMON)
		*(.bss)
		*(.bootstrap_stack)
	}
	/* Add a symbol that indicates the end address of the kernel. */
	_kernel_end = .;

}
