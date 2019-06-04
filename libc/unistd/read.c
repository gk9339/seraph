#include <unistd.h>
#include <errno.h>
#include <sys/syscall.h>
#include <kernel/types.h>

DEFN_SYSCALL3(read, SYS_READ, int, char*,  int)

ssize_t read( int fd, void* ptr, size_t len )
{
    return syscall_read(fd, ptr, len);
}
