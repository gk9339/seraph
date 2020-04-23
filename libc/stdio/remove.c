#include <stdio.h>
#include <unistd.h>

int remove( const char* pathname )
{
    return unlink(pathname);
}
