#include <sys/syscall.h>
#include <sys/ioctl.h>
#include <unistd.h>

DEFN_SYSCALL3(ioctl, SYS_IOCTL, int, int, void*)

int ioctl( int fd, int request, void* argp )
{
    return syscall_ioctl(fd, request, argp);
}

pid_t tcgetpgrp( int fd )
{
    pid_t pgrp;
    ioctl(fd, TIOCGPGRP, &pgrp);

    return pgrp;
}

int tcsetpgrp( int fd, pid_t pgrp )
{
    return ioctl(fd, TIOCSPGRP, &pgrp);
}
