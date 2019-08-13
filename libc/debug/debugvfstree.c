#include <debug.h>
#include <sys/syscall.h>

DEFN_SYSCALL0(debugvfstree, SYS_DEBUGVFSTREE)

int debugvfstree( void )
{
    return(syscall_debugvfstree());
}
