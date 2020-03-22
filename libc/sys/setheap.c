#include <sys/syscall.h>
#include <sys/mman.h>

DEFN_SYSCALL1(setheap, SYS_SETHEAP, uintptr_t)

int setheap( uintptr_t address )
{
    return syscall_setheap(address);
}
