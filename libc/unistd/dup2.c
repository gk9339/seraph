#include <unistd.h>
#include <sys/syscall.h>

DEFN_SYSCALL2(dup2, SYS_DUP2, int, int)

int dup2( int oldfd, int newfd )
{
    return syscall_dup2(oldfd, newfd);
}
