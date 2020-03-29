.section .text
.align 4

.global idt_load
.type idt_load, @function
idt_load:
    # Get pointer to the IDT
	mov 4(%esp), %eax
    # Load new IDT
	lidt (%eax)
	ret
