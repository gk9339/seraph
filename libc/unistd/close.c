#include <unistd.h>
#include <errno.h>
#include <sys/syscall.h>

DEFN_SYSCALL1(close, SYS_CLOSE, int)

int close( int file )
{
    __sets_errno(syscall_close(file));
}
