# Function epilogue for _init
.section .init
    # Restore frame pointer
	popl %ebp
	ret

# Function epilogue for _fini
.section .fini
    # Restore frame pointer
	popl %ebp
	ret
