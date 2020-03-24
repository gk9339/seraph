#include <sys/syscall.h>
#include <sys/mman.h>

DEFN_SYSCALL2(mmap, SYS_MMAP, uintptr_t, size_t)

int mmap( uintptr_t address, size_t size )
{
    return syscall_mmap(address, size);
}
