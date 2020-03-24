#include <unistd.h>
#include <sys/syscall.h>

DEFN_SYSCALL0(getppid, SYS_GETPPID)

pid_t getppid( void )
{
    return syscall_getppid();
}
