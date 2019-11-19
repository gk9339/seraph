#include <unistd.h>
#include <sys/syscall.h>
#include <errno.h>

DEFN_SYSCALL3(write, SYS_WRITE, int, char*, int)

ssize_t write( int file, const void* ptr, size_t len )
{
    __sets_errno(syscall_write(file, (char*)ptr, len));
}
