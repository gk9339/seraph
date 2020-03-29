.section .text
.align 4

# Top of stack pointing to task %eip, move to %eax, return
.global read_eip
.type read_eip, @function
read_eip:
    mov (%esp), %eax
    ret

# For fork/clone returning to ring 3
.global return_to_userspace
.type return_to_userspace, @function
return_to_userspace:
    # Save segment registers onto stack
    pop %gs
    pop %fs
    pop %es
    pop %ds
    # Save all general purpose registers onto stack
    popa
    # Increment stack pointer
    add $8, %esp
    # Return to ring 3
    iret

# Enter userspace at given (location, stack)
.global enter_userspace
.type enter_userspace, @function
.set MAGIC, 0x5A1B83AA
enter_userspace:
    # Preserve current frame pointer
    push %ebp
    mov %esp, %ebp
    mov 0xC(%ebp), %edx
    mov %edx, %esp
    push $MAGIC

    # Segment selector
    mov $0x23, %ax

    # Save segment registers
    mov %eax, %ds
    mov %eax, %es
    mov %eax, %fs
    mov %eax, %gs

    # Store stack address in %eax 
    mov %esp, %eax

    # Data segment with bottom 2 bits set for ring 3
    push $0x23

    # Push the stack address
    push %eax

    # Push flags and fix interrupt flsg
    pushf
    pop %eax

    # Request ring3
    orl $0x200, %eax
    push %eax
    push $0x1B

    # Push entry point
    push 0x8(%ebp)

    # Enter ring 3 (pretending to return to)
    iret
    # Restore frame pointer
    pop %ebp
    ret
