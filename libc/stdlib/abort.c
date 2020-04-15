#include <stdio.h>
#include <stdlib.h>
#include <signal.h>
#include <sys/syscall.h>
#if defined(__is_libk)
#include <kernel/kernel.h>
#endif

void __attribute__((__noreturn__)) abort( void ) 
{
#if defined(__is_libk)
    KPANIC("STDLIB abort()", NULL);
#else
    raise(SIGABRT);
    syscall_exit(-1);
#endif
	while (1) { }
	__builtin_unreachable();
}
