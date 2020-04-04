#include <unistd.h>
#include <errno.h>
#include <sys/syscall.h>

DEFN_SYSCALL1(pipe, SYS_PIPE, int*)

int pipe( int fd[2] )
{
    __sets_errno(syscall_pipe((int*)fd))
}
