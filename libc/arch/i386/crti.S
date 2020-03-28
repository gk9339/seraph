.global _init
.section .init
_init:
	push %ebp
	mov %esp, %ebp

.global _fini
.section .fini
_fini:
	push %ebp
	mov %esp, %ebp
