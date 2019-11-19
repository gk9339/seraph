#include <unistd.h>
#include <sys/syscall.h>

DEFN_SYSCALL0(getpid, SYS_GETPID)

pid_t getpid( void )
{
    return syscall_getpid();
}
