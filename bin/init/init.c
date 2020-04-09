#include <unistd.h>
#include <sys/syscall.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <stdio.h>
#include <stdlib.h>
#include <debug.h>

int main( void )
{
    if( getpid() != 1 )
    {
        printf("Init already started.\nExiting\n");
        exit(0);
    }

    // Setup standard streams to point to /dev/null
    syscall_open("/dev/null", 0, 0);
    syscall_open("/dev/null", 1, 0);
    syscall_open("/dev/null", 1, 0);
    
    // TODO: use /etc/init.d or similar for this
    pid_t pid = fork();

    if(!pid)
    {
        char* arg[] = { "/bin/terminal", NULL };
        char* env[] = { "PATH=/bin", "LD_LIBRARY_PATH=/lib", NULL};
        execve("/bin/terminal", arg, env);
    }

    while ((pid=waitpid(-1,NULL,WNOKERN))!=-1);

    sleep(1);
    syscall_reboot(1);
    __builtin_unreachable();
}
