.section .text
.align 4

.macro IRQ ident byte
    .global _irq\ident
    .type _irq\ident, @function
    _irq\ident:
        cli
        push $0x00
        push $\byte
        jmp irq_common
.endm

# Interrupt requests
IRQ 0, 32 # Programmable Interrupt Timer
IRQ 1, 33 # PS2 Keyboard
IRQ 2, 34 # Cascade (never raised)
IRQ 3, 35 # COM2
IRQ 4, 36 # COM1
IRQ 5, 37 # LPT2
IRQ 6, 38 # Floppy Disk
IRQ 7, 39 # LPT1 (unreliable / spurious)
IRQ 8, 40 # CMOS real-time clock
IRQ 9, 41 # Free for peripherals / legacy SCSI / NIC
IRQ 10, 42 # Free for peripherals / SCSI / NIC
IRQ 11, 43 # Free for peripherals / SCSI / NIC
IRQ 12, 44 # PS2 Mouse
IRQ 13, 45 # FPU / Coprocessor / Inter-processor
IRQ 14, 46 # Primary ATA Hard Disk
IRQ 15, 47 # Secondary ATA Hard Disk

.extern irq_handler
.type irq_handler, @function
irq_common:
    # Save all registers / segment registers
    pusha
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

    # Call handler
    push %esp
    call irq_handler
    add $4, %esp

    # Restore segment / all registers
    pop %gs
    pop %fs
    pop %es
    pop %ds
    popa

    # Cleanup error code/ irq# 
    add $8, %esp

    iret
