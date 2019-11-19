#include <unistd.h>
#include <sys/syscall.h>

DEFN_SYSCALL0(setsid, SYS_SETSID)

pid_t setsid( void )
{
    return syscall_setsid();
}
