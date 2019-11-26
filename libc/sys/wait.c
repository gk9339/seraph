#include <sys/wait.h>
#include <sys/syscall.h>
#include <errno.h>

DEFN_SYSCALL3(waitpid, SYS_WAITPID, int, int*, int)

int waitpid( int pid, int* status, int options )
{
    __sets_errno(syscall_waitpid(pid, status, options));
}

int wait(int* status)
{
    __sets_errno(waitpid(-1, status, 0));
}
