#include <stdio.h>
#include <fcntl.h>
#include <sys/syscall.h>
#include <stdlib.h>
#include <string.h>
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
 

FILE* freopen( const char* path, const char* mode, FILE* stream )
{
    if( path )
    {
        if( stream )
        {
            fclose(stream);
        }
        int flags, mask;
        parse_mode(mode, &flags, &mask);
     
        int fd = syscall_open(path, flags, mask);
     
        if( fd < 0 )
        {
            errno = -fd;
            return NULL;
        }
     
        FILE* out = calloc(1, sizeof(FILE));
     
        out->fd = fd;
        out->read_base = out->read_ptr = out->read_end = NULL;
        out->write_base = out->write_ptr = out->write_end = NULL;
        if( flags & O_RDONLY )
        {
            out->available = 0;
            out->bufmode = -1;
        }else if( flags & O_WRONLY )
        {
            out->available = -1;
            out->bufmode = _IOFBF;
        }else if( flags & O_RDWR )
        {
            out->available = 0;
            out->bufmode = _IOFBF;
        }
        out->ungetc = -1;
        out->eof = 0;
     
        out->_name = strdup(path);
     
        return out;
    }

    return NULL;
}
