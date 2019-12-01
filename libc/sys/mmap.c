#include <sys/syscall.h>

DEFN_SYSCALL1(mmap, SYS_MMAP, size_t);

int mmap( size_t size )
{
    return sys_mmap(size);
}
