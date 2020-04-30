#include <unistd.h>
#include <errno.h>

int rmdir( const char* pathname )
{
    return unlink(pathname);
}
