#include <unistd.h>
#include <sys/syscall.h>
#include <signal.h>
#include <errno.h>

DEFN_SYSCALL2(kill, SYS_KILL, pid_t, int)

int kill( pid_t pid, int sig )
{
    __sets_errno(syscall_kill(pid, sig));
}
