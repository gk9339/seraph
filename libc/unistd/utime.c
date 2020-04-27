#include <unistd.h>
#include <errno.h>

int utime( const char* filename __attribute__((unused)), const struct utimbuf* times __attribute__((unused)) )
{
    errno = ENOTSUP;

    return -1;
}
