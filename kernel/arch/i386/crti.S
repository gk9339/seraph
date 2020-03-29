# Function prolog for _init
.section .init
.global _init
.type _init, @function
_init:
    # Preserve current frame pointer
    push %ebp 
    # Create new frame pointer
    movl %esp, %ebp 
    # Compiler puts crtbegin.o .init section here

# Function prolog for _fini
.section .fini
.global _fini
.type _fini, @function
_fini:
    # Preserve current frame pointer
    push %ebp 
    # Create new frame pointer
    movl %esp, %ebp 
    # Compiler puts crtend.o .init section here
