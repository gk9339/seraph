#include <sys/time.h>
#include <sys/syscall.h>

DEFN_SYSCALL2(gettimeofday, SYS_GETTIMEOFDAY, void*, void*)

int gettimeofday( struct timeval* tv, void* restrict tzp )
{
    return syscall_gettimeofday((void*)tv, tzp);
}
