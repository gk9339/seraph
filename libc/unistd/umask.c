#include <sys/stat.h>
#include <sys/syscall.h>

DEFN_SYSCALL1(umask, SYS_UMASK, int)

mode_t umask( mode_t mask )
{
    return syscall_umask(mask);
}
