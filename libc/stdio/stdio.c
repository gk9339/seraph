#include <stdio.h>
#include <stdlib.h>
#include <sys/syscall.h>
#include <string.h>
#include <errno.h>
#include <fcntl.h>
#include <unistd.h>

struct _FILE
{
    int fd;
    short flags;

    char* read_base; //putback + get
    char* read_ptr;
    char* read_end;
    char* write_base; //put
    char* write_ptr;
    char* write_end;
    
    int available;
    char ungetc;

    int eof;

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
    .ungetc = -1,
    .eof = 0,
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
    .ungetc = -1,
    .eof = 0,
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
    .ungetc = -1,
    .eof = 0,
    ._name = "stderr",
};

FILE* stdin = &_stdin;
FILE* stdout = &_stdout;
FILE* stderr = &_stderr;

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
    out->read_end = out->write_end = out->read_base + BUFSIZ;
    out->write_base = out->write_ptr = out->read_base;

    out->available = 0;
    out->ungetc = -1;
    out->eof = 0;

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
    out->ungetc = -1;
    out->eof = 0;
    
    char tmp[30];
    sprintf(tmp, "fd[%d]", fd);
    out->_name = strdup(tmp);

    return out;
}

int fclose( FILE* stream )
{
    int out = syscall_close(stream->fd);
    free(stream->_name);
    free(stream->read_base);

    if( stream == &_stdin || stream == &_stdout || stream == &_stderr )
    {
        return out;
    }else
    {
        free(stream);
        return out;
    }
}

int fseek( FILE* stream, long offset, int whence )
{
    stream->read_ptr = stream->read_base;
    stream->write_ptr = stream->write_base;
    stream->available = 0;
    stream->ungetc = -1;
    stream->eof = 0;

    int resp = syscall_lseek(stream->fd,offset,whence);
    if( resp < 0 )
    {
        errno = -resp;
        return -1;
    }

    return 0;
}

size_t fread( void* ptr, size_t size, size_t nmemb, FILE* stream )
{
    char* tracking = (char*)ptr;

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

size_t fwrite( const void* ptr, size_t size, size_t count, FILE* stream )
{
    size_t out_size = size * count;

    if( !out_size )
    {
        return 0;
    }

    __sets_errno(syscall_write(stream->fd, (void*)ptr, out_size));
}
