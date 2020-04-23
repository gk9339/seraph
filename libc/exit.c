#include <sys/syscall.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

DEFN_SYSCALL1(exit, SYS_EXT, int)

extern void _fini( void );

void _exit( int val )
{
    fclose(stdin);
    fclose(stdout);
    fclose(stderr);
    _fini();
    syscall_exit(val);

    __builtin_unreachable();
}
