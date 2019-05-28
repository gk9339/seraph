.global _start

_start:
	pop %eax
	push $main
	call main
