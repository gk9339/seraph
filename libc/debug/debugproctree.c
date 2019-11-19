#include <debug.h>
#include <sys/syscall.h>

DEFN_SYSCALL0(debugproctree, SYS_DEBUGPROCTREE)

int debugproctree( void )
{
    return(syscall_debugproctree());
}
