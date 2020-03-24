#include <debug.h>
#include <stdlib.h>
#include <stdio.h>
#include <sys/syscall.h>

DEFN_SYSCALL1(debugproctree, SYS_DEBUGPROCTREE, char**)

int debugproctree( char** str )
{
    return(syscall_debugproctree(str));
}
