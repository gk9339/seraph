#include <unistd.h>
#include <sys/syscall.h>
#include <errno.h>

DEFN_SYSCALL3(readlink, SYS_READLINK, char*, char*, size_t)

ssize_t readlink( const char* path, char* buf, size_t bufsize )
{
    __sets_errno(syscall_readlink((char*)path, buf, bufsize));
}
