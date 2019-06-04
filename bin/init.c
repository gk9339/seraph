#include <unistd.h>
#include <sys/types.h>
#include <stdio.h>

int main( int argc, char** argv )
{
    char* arg[] = { NULL };
    char* env[] = { NULL };
    
    pid_t pid = fork();

    if(!pid)
    {
        execve("bin/terminal", arg, env);
    }
    
    return 0;
}
