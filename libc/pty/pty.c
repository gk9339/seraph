#include <pty.h>
#include <sys/syscall.h>

DEFN_SYSCALL5(openpty, SYS_OPENPTY, int*, int*, char*, void*, void*)

int openpty( int* master, int* slave, char* name, const struct termios* pty_termios, const struct winsize* pty_winsize )
{
    return(syscall_openpty(master, slave, name, (void*)pty_termios, (void*)pty_winsize));
}
