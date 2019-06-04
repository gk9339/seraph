#include <unistd.h>
#include <sys/syscall.h>

DEFN_SYSCALL0(fork, SYS_FORK)

pid_t fork( void )
{
    return syscall_fork();
}
