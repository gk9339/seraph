#include <stdio.h>
#include <fcntl.h>
#include <string.h>
#include <stdlib.h>
#include <sys/syscall.h>
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

FILE* fopen( const char* pathname, const char* mode )
{
    int flags, mask;
    parse_mode(mode, &flags, &mask);

    int fd = syscall_open(pathname, flags, mask);

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

    out->_name = strdup(pathname);

    return out;
}

FILE* fdopen( int fd, const char* mode )
{
    int fdflags, flags, mask;
    //Checks fd is valid
    if( (fdflags = fcntl(fd, F_GETFL)) < 0 )
    {
        errno = EBADF;
        return 0;
    }
    parse_mode(mode, &flags, &mask);
    //Checks mode is a subset of original
    int fdmode = fdflags & O_ACCMODE;
    if( fdmode != O_RDWR && (fdmode != (flags & O_ACCMODE)) )
    {
        errno = EBADF;
        return 0;
    }
    //POSIX reccomends setting O_APPEND to match
    if( (flags & O_APPEND) && !(fdflags & O_APPEND) )
    {
        fcntl(fd, F_SETFL, fdflags | O_APPEND);
    }

    FILE* out = malloc(sizeof(FILE));
    memset(out, 0, sizeof(struct _FILE));

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
    
    char tmp[30];
    sprintf(tmp, "fd[%d]", fd);
    out->_name = strdup(tmp);

    return out;
}

FILE* fmemopen( void* buf, size_t size, const char* mode )
{
    int flags, mask;
    parse_mode(mode, &flags, &mask);

    FILE* out = malloc(sizeof(FILE));
    memset(out, 0, sizeof(struct _FILE));

    out->fd = -1;
    out->read_base = out->read_ptr = out->read_end = NULL;
    out->write_base = out->write_ptr = out->write_end = NULL;
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
