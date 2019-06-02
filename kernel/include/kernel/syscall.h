#ifndef _KERNEL_SYSCALL_H
#define _KERNEL_SYSCALL_H

#include <stdarg.h>
#include <sys/types.h>

typedef uint32_t (*syscall_function_t)(unsigned int, ...);

void syscalls_initialize( void );

#endif
