#define _ASM_FILE
#include <kernel/multiboot.h>

# Declare constants for the multiboot header.
.set ALIGN,    MULTIBOOT_PAGE_ALIGN    # Align loaded modules on page boundaries
.set MEMINFO,  MULTIBOOT_MEMORY_INFO   # Provide memory map
.set VIDEO,    MULTIBOOT_VIDEO_MODE    # Provide video mode
.set FLAGS,    ALIGN | MEMINFO | VIDEO # Multiboot 'flag' field
.set MAGIC,    0x1BADB002              # 'magic number' lets bootloader find the header
.set CHECKSUM, -(MAGIC + FLAGS)        # Checksum of above, to prove we are Multiboot

# Declare a header as in the Multiboot standard.
.section .multiboot
.align 4
.long MAGIC
.long FLAGS
.long CHECKSUM
.long 0, 0, 0, 0, 0 # Only used in aout, set 0 for elf
.long 0    # Video mode
.long 1280 # Width
.long 720  # Height
.long 32   # Depth

# Allocate the initial stack
.section .kernel_stack, "aw", @nobits
stack_bottom:
.skip 16384 # 16 KiB
stack_top:

# The kernel entry point
.section .text
.global _start
.type _start, @function
_start:
    # Set up the stack
    mov $stack_top, %esp

    # Push the pointer to the stack, multiboot struct and magic value
    pushl %esp
    pushl %ebx
    pushl %eax

    # Enter the high-level kernel
    cli
    call kernel_main

    # Infinite loop if the system has nothing more to do
    cli
1:  hlt
    jmp 1b
