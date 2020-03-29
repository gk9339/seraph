.section .text
.align 4

.macro ISR_NOERR index
    .global _isr\index
    _isr\index:
        cli
        push $0
        push $\index
        jmp isr_common
.endm

.macro ISR_ERR index
    .global _isr\index
    _isr\index:
        cli
        push $\index
        jmp isr_common
.endm

# Standard x86 interrupt service routines
ISR_NOERR 0 # Divide by 0
ISR_NOERR 1 # Reserved
ISR_NOERR 2 # NMI Interrupt
ISR_NOERR 3 # Breakpoint (INT3)
ISR_NOERR 4 # Overflow (INTO)
ISR_NOERR 5 # Bounds range exceeded (BOUND)
ISR_NOERR 6 # Invalid opcode (UD2)
ISR_NOERR 7 # Device not available (WAIT/FWAIT)
ISR_ERR 8 # Double Fault
ISR_NOERR 9 # Coprocessor segment overrun
ISR_ERR 10 # Invalid TSS
ISR_ERR 11 # Segment not present
ISR_ERR 12 # Stack-segment fault
ISR_ERR 13 # General protection fault
ISR_ERR 14 # Page fault
ISR_NOERR 15 # Reserved
ISR_NOERR 16 # x87 FPU error
ISR_NOERR 17 # Alignment check
ISR_NOERR 18 # Machine check
ISR_NOERR 19 # SIMD Floating-Point Exception
ISR_NOERR 20 # Reserved
ISR_NOERR 21 # Reserved
ISR_NOERR 22 # Reserved
ISR_NOERR 23 # Reserved
ISR_NOERR 24 # Reserved
ISR_NOERR 25 # Reserved
ISR_NOERR 26 # Reserved
ISR_NOERR 27 # Reserved
ISR_NOERR 28 # Reserved
ISR_NOERR 29 # Reserved
ISR_NOERR 30 # Reserved
ISR_NOERR 31 # Reserved
ISR_NOERR 127 # Syscall Vector

.extern fault_handler
.type fault_handler, @function
isr_common:
    # Push all registers
    pusha

    # Save segment registers
    push %ds
    push %es
    push %fs
    push %gs
    mov $0x10, %ax
    mov %ax, %ds
    mov %ax, %es
    mov %ax, %fs
    mov %ax, %gs
    cld

    # Call fault handler
    push %esp
    call fault_handler
    add $4, %esp

    # Restore segment registers
    pop %gs
    pop %fs
    pop %es
    pop %ds

    # Restore registers
    popa

    # cleamup error code / ISR#
    add $8, %esp

    # pop CS, EIP, EFLAGS, SS and ESP
    iret
