#include <sys/syscall.h>
#include <errno.h>
#include <sched.h>

DEFN_SYSCALL0(yield, SYS_YIELD)

int sched_yield( void )
{
    return syscall_yield();
}
