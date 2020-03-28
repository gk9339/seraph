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
