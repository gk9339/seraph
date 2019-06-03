#ifndef _IOCTL_H
#define _IOCTL_H

#include <sys/termios.h>

#define IOCTLDTYPE 0x4F00

#define IOCTL_DTYPE_UNKNOWN -1
#define IOCTL_DTYPE_FILE     1
#define IOCTL_DTYPE_TTY      2

#define IOCTLTTYNAME  0x4F01
#define IOCTLTTYLOGIN 0x4F02

#define IOCTL_PACKETFS_QUEUED 0x5050

struct winsize
{
	unsigned short ws_row;
	unsigned short ws_col;
	unsigned short ws_xpixel;
	unsigned short ws_ypixel;
};

#endif
