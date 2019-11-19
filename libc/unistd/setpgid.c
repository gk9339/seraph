#include <unistd.h>
#include <sys/syscall.h>
#include <errno.h>

DEFN_SYSCALL2(setpgid, SYS_SETPGID, int, int)
DEFN_SYSCALL1(getpgid, SYS_GETPGID, int)

int setpgid( pid_t pid, pid_t pgid )
{
    __sets_errno(syscall_setpgid(pid, pgid));
}

gid_t getpgid( pid_t pid )
{
    __sets_errno(syscall_getpgid(pid));
}
