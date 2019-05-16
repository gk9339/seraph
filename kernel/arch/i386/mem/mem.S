.section .text
.align 4

# Disable paging
.set dp, 0x7FFFFFFF

# Enable paging
.set ep, 0x80000000

.global copy_page_physical
.type copy_page_physical, @function
copy_page_physical:
	# Preserve initial contents
	push %ebx 
	pushf
	cli

	# Load source and dest addresses
	mov 12(%esp), %ebx
	mov 16(%esp), %ecx

	# Get control register and disable paging
	mov %cr0, %edx
	and $dp, %edx
	mov %edx, %cr0

	# Copy 4096 bytes
	mov $0x400, %edx
.page_loop:
	# Get word at source address
	mov (%ebx), %eax

	# Store it at dest address
	mov %eax, (%ecx)

	# Increment source and dest addresses to next word
	add $4, %ebx
	add $4, %ecx

	# Decrement words to copy
	dec %edx
	jnz .page_loop

	# Get control register and enable paging
	mov %cr0, %edx
	or $ep, %edx
	mov %edx, %cr0

	# Restore EFLAGS and EBX
	popf
	pop %ebx
	ret
