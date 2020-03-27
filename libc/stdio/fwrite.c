#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <errno.h>
#include <sys/syscall.h>
#include "file.h"

static int memcpy_until_newline( void* restrict dstptr, const void* restrict srcptr, size_t size )
{
    unsigned char* dst = (unsigned char*)dstptr;
    const unsigned char* src = (const unsigned char*)srcptr;

    for( size_t i = 0; i < size; i++ )
    {
        dst[i] = src[i];
        if( dst[i] == '\n' )
        {
            return i;
        }
    }

    return size;
}

size_t fwrite( const void* ptr, size_t size, size_t nmemb, FILE* stream )
{
    uintptr_t curptr = (uintptr_t)ptr;
    size_t out_size = size * nmemb;
    size_t retval = 0;

    if( stream == NULL || stream->bufmode == -1 )
    {
        errno = EBADF;
        return 0;
    }

    if( stream->write_base == NULL )
    {
        stream->write_base = stream->write_ptr = malloc(BUFSIZ);
        stream->write_end = stream->write_base + BUFSIZ;
    }
    size_t buffer_size = stream->write_ptr - stream->write_base;
    size_t buffer_space = stream->write_end - stream->write_ptr;

    if( !out_size )
    {
        return 0;
    }

    switch( stream->bufmode )
    {
        case _IOFBF:
            if( out_size < buffer_space )
            {
                memcpy(stream->write_ptr, ptr, out_size);
                stream->write_ptr += out_size;
                return out_size;
            }
            while( out_size != 0 )
            {
                memcpy(stream->write_ptr, (void*)curptr, buffer_space);
                out_size -= buffer_space;
                retval += buffer_space;
                curptr += buffer_space;
                buffer_size = stream->write_ptr - stream->write_base;
                if( buffer_size == 0 )
                {
                    fflush(stream);
                    buffer_size = 0;
                    buffer_space = stream->write_end - stream->write_ptr;    
                }
            }
            return retval;
        case _IOLBF:
            if( out_size < buffer_space )
            {
                out_size = memcpy_until_newline(stream->write_ptr, ptr, out_size);
                stream->write_ptr += out_size;
                return out_size;
            }
            while( out_size != 0 )
            {
                retval += memcpy_until_newline(stream->write_ptr, (void*)curptr, buffer_space);
                out_size -= buffer_space;
                retval += buffer_space;
                curptr += buffer_space;
                buffer_size = stream->write_ptr - stream->write_base;
                if( buffer_size == 0 )
                {
                    fflush(stream);
                    buffer_size = 0;
                    buffer_space = stream->write_end - stream->write_ptr;    
                }
            }
            return retval;
        case _IONBF: ;
            __sets_errno(syscall_write(stream->fd, (void*)ptr, out_size));
    }

    errno = EBADF;
    return 0;
}
