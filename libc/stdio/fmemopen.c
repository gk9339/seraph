#include <stdio.h>
#include <fcntl.h>
#include <string.h>
#include <stdlib.h>
#include <errno.h>
#include "file.h"

static void parse_mode( const char* mode, int* flags_, int* mask_ )
{
    const char* x = mode;

    int flags = 0;
    int mask = 0644;

    while( *x )
    {
        if( *x == 'a' )
        {
            flags |= O_WRONLY;
            flags |= O_APPEND;
            flags |= O_CREAT;
        }

        if( *x == 'w' )
        {
            flags |= O_WRONLY;
            flags |= O_CREAT;
            flags |= O_TRUNC;
            mask = 0666;
        }

        if( *x == '+' )
        {
            flags |= O_RDWR;
            flags &= ~(O_APPEND);
        }
        x++;
    }

    *flags_ = flags;
    *mask_ = mask;
}

FILE* fmemopen( void* buf, size_t size, const char* mode )
{
    int flags, mask;
    parse_mode(mode, &flags, &mask);

    FILE* out = malloc(sizeof(FILE));
    memset(out, 0, sizeof(struct _FILE));

    out->fd = -1;
    if( buf == NULL )
    {
        if( flags & O_RDONLY )
        {
            out->read_base = out->read_ptr = malloc(size);
            out->read_end = out->read_base + size;
            out->available = size;
            out->bufmode = -1;
        }else if( flags & O_WRONLY )
        {
            out->write_base = out->write_ptr = malloc(size);
            out->write_end = out->write_base + size;
            out->available = -1;
            out->bufmode = _IOFBF;
        }else if( flags & O_RDWR )
        {
            out->read_base = out->read_ptr = malloc(size);
            out->read_end = out->read_base + size;
            out->write_base = out->write_ptr = malloc(size);
            out->write_end = out->write_base + size;
            out->available = 0;
            out->bufmode = _IOFBF;
        }
    }else
    {
        if( flags & O_RDONLY )
        {
            out->read_base = out->read_ptr = buf;
            out->read_end = out->read_base + size;
            out->available = size;
            out->bufmode = -1;
        }else if( flags & O_WRONLY )
        {
            out->write_base = out->write_ptr = buf;
            out->write_end = out->write_base + size;
            out->available = -1;
            out->bufmode = _IOFBF;
        }else if( flags & O_RDWR )
        {
            out->read_base = out->read_ptr = buf;
            out->read_end = out->read_base + size;
            out->write_base = out->write_ptr = buf;
            out->write_end = out->write_base + size;
            out->available = 0;
            out->bufmode = _IOFBF;
        }
        if( flags & O_APPEND )
        {
            if( flags & O_RDONLY || flags & O_RDWR )
            {
                out->read_ptr = out->read_base + strnlen(buf, size);
            }
            if( flags & O_WRONLY || flags & O_RDWR )
            {
                out->write_ptr = out->write_base + strnlen(buf, size);
            }
        }
    }
    out->ungetc = -1;
    out->eof = 0;
    out->_name = NULL;

    return out;
}
