.section .text
.align 4

.global read_eip
.type read_eip, @function
read_eip:
	mov (%esp), %eax
	ret

.global return_to_userspace
.type return_to_userspace, @function
return_to_userspace:
    pop %gs
    pop %fs
    pop %es
    pop %ds
    popa
    add $8, %esp
    iret

.global enter_userspace
.type enter_userspace, @function
.set MAGIC, 0x5A1B83AA
enter_userspace:
    pushl %ebp
    mov %esp, %ebp
    mov 0xC(%ebp), %edx
    mov %edx, %esp
    pushl MAGIC

    # Segment selector
    mov $0x23, %ax

    # Save segment registers
    mov %eax, %ds
    mov %eax, %es
    mov %eax, %fs
    mov %eax, %gs

    # Store stack address in %eax 
    mov %eax, %eax

    # Data segment with bottom 2 bits set for ring 3
    pushl $0x23

    # Push the stack address
    pushl %eax

    # Push flags and fix interrupt flsg
    pushf
    popl %eax

    # Request ring3
    orl $0x200, %eax
    pushl %eax
    pushl $0x1B

    # Push entry point
    pushl 0x8(%ebp)

    iret
    popl %ebp
    ret
