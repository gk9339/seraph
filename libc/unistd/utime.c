#include <unistd.h>
#include <errno.h>

int utime( const char* filename, const struct utimbuf* times )
{
    errno = ENOTSUP;

    return -1;
}
