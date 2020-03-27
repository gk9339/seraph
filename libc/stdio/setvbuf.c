#include <stdio.h>
#include <stdlib.h>
#include <errno.h>
#include "file.h"

void setbuf( FILE* stream, char* buf )
{
    setvbuf(stream, buf, buf ? _IOFBF : _IONBF, BUFSIZ);
}

void setbuffer( FILE* stream, char* buf, size_t size )
{
    setvbuf(stream, buf, buf ? _IOFBF : _IONBF, size);
}

void setlinebuf( FILE* stream )
{
    setvbuf(stream, NULL, _IOLBF, 0);;
}

int setvbuf( FILE* stream, char* buffer, int mode, size_t size )
{
    if( stream == NULL )
    {
        errno = EBADF;
        return errno;
    }
    if( mode != _IOFBF &&
        mode != _IOLBF &&
        mode != _IONBF )
    {
        errno = EINVAL;
        return errno;
    }

    stream->bufmode = mode;
    if( buffer && mode != _IONBF )
    {
        if( stream->write_base )
        {
            free(stream->write_base);
        }
        stream->write_base = stream->write_ptr = buffer;
        stream->write_end = buffer + size;
    }

    return 0;
}
