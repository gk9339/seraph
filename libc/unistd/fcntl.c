#include <fcntl.h>
#include <sys/syscall.h>
#include <errno.h>

DEFN_SYSCALL3(fcntl, SYS_FCNTL, int, int, va_list)

int fcntl( int fd, int cmd, ... )
{
    va_list args;
    va_start(args, cmd);
    int ret = syscall_fcntl(fd, cmd, args);
    va_end(args);
    
    if( ret < 0 )
    {
        errno = -ret;
        ret = -1;
    }

    return ret;
}
