#define _ASM_FILE
#include <kernel/multiboot.h>

# Declare constants for the multiboot header.
.set ALIGN,    MULTIBOOT_PAGE_ALIGN 	# align loaded modules on page boundaries
.set MEMINFO,  MULTIBOOT_MEMORY_INFO 	# provide memory map
.set VIDEO,    MULTIBOOT_VIDEO_MODE 	# provide video mode
.set FLAGS,    ALIGN | MEMINFO | VIDEO	# Multiboot 'flag' field
.set MAGIC,    0x1BADB002       		# 'magic number' lets bootloader find the header
.set CHECKSUM, -(MAGIC + FLAGS) 		# checksum of above, to prove we are multiboot

# Declare a header as in the Multiboot Standard.
.section .multiboot
.align 4
.long MAGIC
.long FLAGS
.long CHECKSUM
.long 0, 0, 0, 0, 0 # Only used in aout, set 0 for elf
.long 1    # Video mode
.long 80 # Width
.long 25  # Height
.long 16   # Depth

# Allocate the initial stack
.section .kernel_stack, "aw", @nobits
stack_bottom:
.skip 16384 # 16 KiB
stack_top:

# The kernel entry point.
.section .text
.global _start
.type _start, @function
_start:
	# Set up the stack
	mov $stack_top, %esp
	and $-16, %esp

	# Push the pointer to the stack, multiboot struct and magic value
	pushl %esp
    pushl %ebx
	pushl %eax

	# Enter the high-level kernel
	cli
	call kernel_main

	# Infinite loop if the system has nothing more to do
	cli
1:	hlt
	jmp 1b
