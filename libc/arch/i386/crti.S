# Function prologue for _init
.global _init
.section .init
_init:
    # Preserve current frame pointer
	push %ebp
    # Create new frame pointer
	mov %esp, %ebp
    # Compiler puts crtbegin.o .init section here

# Function prologue for _fini
.global _fini
.section .fini
_fini:
    # Preserve current frame pointer
	push %ebp
    # Create new frame pointer
	mov %esp, %ebp
    # Compiler puts crtend.o .init section here

# Dynamic shared object handle for global destruction (C++)
.section .data
	.global __dso_handle
	.weak __dso_handle
	.hidden __dso_handle
__dso_handle:
	.long 0
