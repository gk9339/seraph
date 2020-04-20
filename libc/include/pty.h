#ifndef _PTY_H
#define _PTY_H

#ifdef __cplusplus
extern "C" {
#endif

#include <sys/termios.h>
#include <sys/ioctl.h>

int openpty( int* master, int* slave, char* name, const struct termios* pty_termios, const struct winsize* pty_winsize );

#ifdef __cplusplus
}
#endif

#endif
