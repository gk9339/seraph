#include <stdio.h>
#include <errno.h>
#include <sys/syscall.h>
#include "file.h"

int fflush( FILE* stream )
{
    if( stream == NULL || stream->write_base == NULL )
    {
        errno = EBADF;
        return 1;
    }
    if( stream->write_ptr == stream->write_base )
    {
        return 0;
    }

    size_t size = stream->write_ptr - stream->write_base;
    stream->write_ptr = stream->write_base;
    __sets_errno(syscall_write(stream->fd, (void*)stream->write_base, size));
}
