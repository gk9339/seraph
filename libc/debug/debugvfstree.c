#include <debug.h>
#include <sys/syscall.h>

DEFN_SYSCALL1(debugvfstree, SYS_DEBUGVFSTREE, char**)

int debugvfstree( char** str )
{
    return(syscall_debugvfstree(str));
}
