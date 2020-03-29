.section .text
.align 4

# Takes first parameter (GDT), loads this value into GDT, then loads segment selectors
# for the data and code segments.
.global gdt_flush
.type gdt_flush, @function
gdt_flush:
    # Get pointer to the GDT
    mov 4(%esp), %eax
    # Load new GDT
    lgdt (%eax)

    # 0x10 is the offset in the GDT to the data segment
    mov $0x10, %ax
    # Load all segment selectors
    mov %ax, %ds
    mov %ax, %es
    mov %ax, %fs
    mov %ax, %ss
    mov %ax, %gs

    ljmp $0x08, $.flush

.flush:
    ret
