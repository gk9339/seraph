#include <unistd.h>

pid_t fork( void )
{
    return 0;
}

int execv( const char* pathname, char* const argv[] )
{
    return 0;
}

int execve( const char* pathname, char* const argv[], char* const envp[] )
{
    return 0;
}

int execvp( const char* filename, char* const argv[] )
{
    return 0;
}