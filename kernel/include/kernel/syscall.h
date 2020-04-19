#ifndef _KERNEL_SYSCALL_H
#define _KERNEL_SYSCALL_H

#include <stdint.h> // intN_t

typedef uint32_t (*syscall_function_t)(unsigned int, ...);

void syscalls_initialize( void );
void ptr_validate( void* ptr, const char* syscall );

#endif
