.global _start

_start:
	pop %eax
	push $main
	call _pre_main
