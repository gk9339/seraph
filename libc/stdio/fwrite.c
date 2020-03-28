#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <errno.h>
#include <sys/syscall.h>
#include <stdbool.h>
#include "file.h"

size_t fwrite( const void* ptr, size_t size, size_t nmemb, FILE* stream )
{
    uintptr_t curptr = (uintptr_t)ptr;
    size_t out_size = size * nmemb;
    size_t buffer_space;

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

    if( !out_size )
    {
        return 0;
    }

#ifdef __is_libk
    while( out_size )
    {
        buffer_space = stream->write_end - stream->write_ptr;
        if( out_size < buffer_space )
        {
            for( size_t i = 0; i < out_size; i++ )
            {
                *stream->write_ptr = *(char*)curptr;
                stream->write_ptr++;
                curptr++;
            }
            out_size = 0;
        }else
        {
            return -1;
        }
        if( stream->eof )
        {
            break;
        }
    }
    return ((size * nmemb) - out_size) / size;
#else
    switch( stream->bufmode )
    {
        case _IOFBF:
            while( out_size )
            {
                buffer_space = stream->write_end - stream->write_ptr;
                if( out_size < buffer_space )
                {
                    for( size_t i = 0; i < out_size; i++ )
                    {
                        *stream->write_ptr = *(char*)curptr;
                        stream->write_ptr++;
                        curptr++;
                    }
                    out_size = 0;
                }else
                {
                    for( size_t i = 0; i < out_size; i++ )
                    {
                        *stream->write_ptr = *(char*)curptr;
                        stream->write_ptr++;
                        curptr++;
                    }
                    out_size -= buffer_space;
                    fflush(stream);
                    
                }
                if( stream->eof )
                {
                    break;
                }
            }
            return ((size * nmemb) - out_size) / size;
        case _IOLBF:
            while( out_size )
            {
                buffer_space = stream->write_end - stream->write_ptr;
                if( out_size < buffer_space )
                {
                    for( size_t i = 0; i < out_size; i++ )
                    {
                        *stream->write_ptr = *(char*)curptr;
                        stream->write_ptr++;
                        if( *(stream->write_ptr - 1) == '\n' )
                        {
                            fflush(stream);
                        }
                        curptr++;
                    }
                    out_size = 0;
                }else
                {
                    for( size_t i = 0; i < out_size; i++ )
                    {
                        *stream->write_ptr = *(char*)curptr;
                        stream->write_ptr++;
                        if( *(stream->write_ptr - 1) == '\n' )
                        {
                            fflush(stream);
                        }
                        curptr++;
                    }
                    out_size -= buffer_space;
                    fflush(stream);
                }
                if( stream->eof )
                {
                    break;
                }
            }
            return ((size * nmemb) - out_size) / size;
        case _IONBF: ;
            int retval = syscall_write(stream->fd, (void*)ptr, out_size);
            if( retval < 0 )
            {
                errno = -retval;
                return 0;
            }else
            {
                return nmemb;
            }

    }
#endif

    errno = EBADF;
    return 0;
}
