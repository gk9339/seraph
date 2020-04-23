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

.section .data
	.global __dso_handle
	.weak __dso_handle
	.hidden __dso_handle
__dso_handle:
	.long 0
