.section .text
.align 4

.global tss_flush
.type tss_flush, @function
tss_flush:
    # Get pointer to TSS
    mov $0x2B, %ax
    # Load new TSS
    ltr %ax
    ret
