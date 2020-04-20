#ifndef _FCNTL_H
#define _FCNTL_H

#ifdef __cplusplus
extern "C" {
#endif

#include <sys/types.h>

#define FD_CLOEXEC   0x0001

#define F_GETFD 1
#define F_SETFD 2

#define O_RDONLY     0x0000
#define O_WRONLY     0x0001
#define O_RDWR       0x0002
#define O_ACCMODE    0x0003
#define O_APPEND     0x0008
#define O_CREAT      0x0100
#define O_EXCL       0x0200
#define O_NOCTTY     0x0400
#define O_NOFOLLOW   0x0800
#define O_TRUNC      0x1000
#define O_PATH       0x2000
#define O_NONBLOCK   0x4000
#define O_DIRECTORY  0x8000

#define F_GETFL 3
#define F_SETFL 4

#define F_OK 1
#define R_OK 4
#define W_OK 2
#define X_OK 1

int open (const char *, int, ...);
int fcntl( int fd, int cmd, ... );

#ifdef __cplusplus
}
#endif

#endif
