#include <unistd.h>
#include <errno.h>
#include <sys/syscall.h>

DEFN_SYSCALL3(lseek, SYS_SEEK, int, int, int)

off_t lseek( int fd, off_t offset, int whence )
{
    __sets_errno(syscall_lseek(fd, offset, whence));
}
