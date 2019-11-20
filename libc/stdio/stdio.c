#include <stdio.h>
#include <stdlib.h>
#include <sys/syscall.h>
#include <string.h>
#include <errno.h>

struct _FILE
{
    int fd;

    char* read_base;
    char* read_ptr;
    char* read_end;
    char* write_base;
    char* write_ptr;
    char* write_end;
    char* buf_base;
    char* buf_end;
    
    int available;

    char* _name;
};

FILE _stdin =
{
    .fd = 0,
    .read_base = NULL,
    .read_ptr = NULL,
    .read_end = NULL,
    .write_base = NULL,
    .write_ptr = NULL,
    .write_end = NULL,
    .buf_base = NULL,
    .buf_end = NULL,
    .available = 0,
    ._name = "stdin",
};

FILE _stdout =
{
	.fd = 1,
    .read_base = NULL,
    .read_ptr = NULL,
    .read_end = NULL,
    .write_base = NULL,
    .write_ptr = NULL,
    .write_end = NULL,
    .buf_base = NULL,
    .buf_end = NULL,
    .available = 0,
    ._name = "stdout",
};

FILE _stderr =
{
	.fd = 2,
    .read_base = NULL,
    .read_ptr = NULL,
    .read_end = NULL,
    .write_base = NULL,
    .write_ptr = NULL,
    .write_end = NULL,
    .buf_base = NULL,
    .buf_end = NULL,
    .available = 0,
    ._name = "stderr",
};

FILE* stdin = &_stdin;
FILE* stdout = &_stdout;
FILE* stderr = &_stderr;

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
    out->buf_base = malloc(BUFSIZ);
    out->buf_end = out->buf_base + BUFSIZ;
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
