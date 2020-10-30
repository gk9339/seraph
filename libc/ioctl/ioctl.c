#include <sys/syscall.h>
#include <sys/ioctl.h>
#include <unistd.h>

DEFN_SYSCALL3(ioctl, SYS_IOCTL, int, int, void*)

int ioctl( int fd, int request, void* argp )
{
    return syscall_ioctl(fd, request, argp);
}

int tcgetattr( int fd, struct termios* termios_p )
{
    return ioctl(fd, TCGETS, termios_p);
}

int tcsetattr( int fd, int actions, struct termios* termios_p )
{
    switch( actions )
    {
        case TCSANOW:
            return ioctl(fd, TCSETS, termios_p);
        case TCSADRAIN:
            return ioctl(fd, TCSETSW, termios_p);
        case TCSAFLUSH:
            return ioctl(fd, TCSETSF, termios_p);
        default:
            return 0;
    }
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
