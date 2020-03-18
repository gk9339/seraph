#include <sys/syscall.h>
#include <sys/mman.h>

DEFN_SYSCALL1(mmap, SYS_MMAP, size_t)

int mmap( size_t size )
{
    return syscall_mmap(size);
}
