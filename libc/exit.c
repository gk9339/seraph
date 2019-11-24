#include <sys/syscall.h>
#include <stdio.h>
#include <stdlib.h>

DEFN_SYSCALL1(exit, SYS_EXT, int)

extern void _fini( void );

void _exit( int val );

void _exit( int val )
{
    _fini();
    syscall_exit(val);

    __builtin_unreachable();
}
