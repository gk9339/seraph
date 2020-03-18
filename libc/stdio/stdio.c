#include <stdio.h>
#include <stdlib.h>
#include <sys/syscall.h>
#include <string.h>
#include <errno.h>
#include <fcntl.h>

struct _FILE
{
    int fd;
    short flags;

    char* read_base;
    char* read_ptr;
    char* read_end;
    char* write_base;
    char* write_ptr;
    char* write_end;
    
    int available;

    char* _name;
};

FILE _stdin =
{
    .fd = 0,
    .flags = 0,
    .read_base = NULL,
    .read_ptr = NULL,
    .read_end = NULL,
    .write_base = NULL,
    .write_ptr = NULL,
    .write_end = NULL,
    .available = 0,
    ._name = "stdin",
};

FILE _stdout =
{
	.fd = 1,
    .flags = 0,
    .read_base = NULL,
    .read_ptr = NULL,
    .read_end = NULL,
    .write_base = NULL,
    .write_ptr = NULL,
    .write_end = NULL,
    .available = 0,
    ._name = "stdout",
};

FILE _stderr =
{
	.fd = 2,
    .flags = 0,
    .read_base = NULL,
    .read_ptr = NULL,
    .read_end = NULL,
    .write_base = NULL,
    .write_ptr = NULL,
    .write_end = NULL,
    .available = 0,
    ._name = "stderr",
};

FILE* stdin = &_stdin;
FILE* stdout = &_stdout;
FILE* stderr = &_stderr;

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
    out->read_base = out->read_ptr = malloc(BUFSIZ);
    out->read_end = out->read_base + BUFSIZ;
    out->write_base = out->read_ptr = malloc(BUFSIZ);
    out->write_end = out->read_base + BUFSIZ;

    out->available = 0;

    out->_name = strdup(pathname);

    return out;
}

FILE* fdopen( int fd, const char* mode )
{
    FILE* out = malloc(sizeof(FILE));
    memset(out, 0, sizeof(struct _FILE));

    out->fd = fd;
    out->read_base = NULL,
    out->read_ptr = NULL,
    out->read_end = NULL,
    out->write_base = NULL,
    out->write_ptr = NULL,
    out->write_end = NULL,
    
    out->available = 0;
    
    char tmp[30];
    sprintf(tmp, "fd[%d]", fd);
    out->_name = strdup(tmp);

    return out;
}

size_t fwrite( const void* ptr, size_t size, size_t count, FILE* stream )
{
    size_t out_size = size * count;

    if( !out_size )
    {
        return 0;
    }

    __sets_errno(syscall_write(stream->fd, (void*)ptr, out_size));
}
