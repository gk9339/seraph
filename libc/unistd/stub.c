#include <unistd.h>

#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wunused-parameter"

int execv( const char* pathname, char* const argv[] )
{
    return 0;
}

int execvp( const char* filename, char* const argv[] )
{
    return 0;
}

#pragma GCC diagnostic pop
