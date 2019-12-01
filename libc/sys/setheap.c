#include <sys/syscall.h>

DEFN_SYSCALL1(setheap, SYS_SETHEAP, uintptr_t)

int setheap( uintptr_t address )
{
    return sys_setheap(address);
}
