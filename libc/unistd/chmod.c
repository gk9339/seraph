#include <fcntl.h>
#include <sys/syscall.h>
#include <errno.h>

DEFN_SYSCALL2(chmod, SYS_CHMOD, char*, int)

int chmod( const char* path, mode_t mode )
{
    __sets_errno(syscall_chmod((char*)path, mode));
}
