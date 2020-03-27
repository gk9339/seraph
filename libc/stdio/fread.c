#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <errno.h>
#include "file.h"

int fileno( FILE* stream )
{
    return stream->fd;
}

static size_t read_bytes( FILE* f, char* out, size_t len )
{
    size_t r_out = 0;
    
    while( len > 0 )
    {
        if( f->ungetc >= 0 )
        {
            *out = f->ungetc;
            len--;
            out++;
            r_out++;
            f->ungetc = -1;
            continue;
        }

        if( f->available == 0 )
        {
            if( f->read_ptr == f->read_end )
            {
                f->read_ptr = f->read_base;
            }
            ssize_t r = read(fileno(f), f->read_ptr, (f->read_end - f->read_ptr));
            if( r < 0 )
            {
                return r_out;
            }else
            {
                f->available = r;

            }
        }

        if( f->available == 0 )
        {
            f->eof = 1;
            return r_out;
        }

        while( f->read_ptr < f->read_end && len > 0 && f->available > 0 )
        {
            *out = *f->read_ptr;
            len--;
            f->read_ptr++;
            f->available--;
            out++;
            r_out++;
        }
    }
    
    return r_out;
}

size_t fread( void* ptr, size_t size, size_t nmemb, FILE* stream )
{
    char* tracking = (char*)ptr;
    if( stream == NULL || stream->available == -1 )
    {
        errno = EBADF;
        return 0;
    }

    if( stream->read_base == NULL )
    {
        stream->read_base = stream->read_ptr = malloc(BUFSIZ);
        stream->read_end = stream->read_base + BUFSIZ;
    }

    for( size_t i = 0; i < nmemb; i++ )
    {
        int r = read_bytes(stream, tracking, size);
        if( r < 0 )
        {
            return -1;
        }
        tracking += r;
        if( r < (int)size )
        {
            return i;
        }
    }

    return nmemb;
}
