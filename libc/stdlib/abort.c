#include <stdio.h>
#include <stdlib.h>
#include <sys/syscall.h>
#if defined(__is_libk)
#include <kernel/kernel.h>
#endif

__attribute__((__noreturn__))
void abort( void ) 
{
#if defined(__is_libk)
    KPANIC("STDLIB abort()", NULL);
#else
    syscall_exit(-1);
#endif
	while (1) { }
	__builtin_unreachable();
}
