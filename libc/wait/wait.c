#include <sys/wait.h>
#include <sys/syscall.h>

DEFN_SYSCALL3(waitpid, SYS_WAITPID, int, int*, int)

int waitpid( int pid, int* status, int options )
{
    return syscall_waitpid(pid, status, options);
}

int wait(int* status)
{
    return waitpid(-1, status, 0);
}
