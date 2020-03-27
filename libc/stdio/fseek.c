#include <stdio.h>
#include <errno.h>
#include <sys/syscall.h>
#include "file.h"

int fseek( FILE* stream, long offset, int whence )
{
    if( stream == NULL )
    {
        errno = EBADF;
        return -1;
    }

    if( stream->available != -1 )
    {
        stream->read_base = stream->read_ptr = stream->read_end = NULL;
        stream->available = 0;
    }
    if( stream->bufmode != -1 )
    {
        stream->write_base = stream->write_ptr = stream->write_end = NULL;
    }
    stream->ungetc = -1;
    stream->eof = 0;

    int resp = syscall_lseek(stream->fd,offset,whence);
    if( resp < 0 )
    {
        errno = -resp;
        return -1;
    }

    return 0;
}
