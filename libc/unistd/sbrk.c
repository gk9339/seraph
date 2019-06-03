#include <unistd.h>
#include <sys/syscall.h>
#include <sys/types.h>

DEFN_SYSCALL1(sbrk, SYS_SBRK, int)

void* sbrk( intptr_t increment )
{
    return (void*)syscall_sbrk(increment);
}
