#include <unistd.h>
#include <sys/syscall.h>
#include <sys/types.h>
#include <stdio.h>

int main( void )
{
    syscall_open("/dev/null", 0, 0);
    syscall_open("/dev/null", 1, 0);
    syscall_open("/dev/null", 1, 0);
    
    pid_t pid = fork();

    if(!pid)
    {
        char* arg[] = { NULL };
        char* env[] = { NULL };
        execve("/bin/terminal", arg, env);
    }

    return 0;
}
