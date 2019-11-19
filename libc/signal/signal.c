#include <sys/syscall.h>
#include <signal.h>

DEFN_SYSCALL2(signal, SYS_SIGNAL, uint32_t, sighandler_t*)

sighandler_t signal(int signum, sighandler_t handler)
{
    return (sighandler_t)syscall_signal(signum, &handler);
}
