#include <unistd.h>
#include <sys/syscall.h>

DEFN_SYSCALL0(getgid, SYS_GETGID)

uid_t getgid( void )
{
    return syscall_getgid();
}
