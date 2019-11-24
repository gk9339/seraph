#include <sys/syscall.h>
#include <sys/fswait.h>
#include <errno.h>

DEFN_SYSCALL2(fswait, SYS_FSWAIT, int, int*)
DEFN_SYSCALL3(fswait2, SYS_FSWAIT2, int, int*, int)

int fswait( int count, int* fds )
{
    __sets_errno(syscall_fswait(count, fds));
}

int fswait2( int count, int* fds, int timeout )
{
    __sets_errno(syscall_fswait2(count, fds, timeout));
}
