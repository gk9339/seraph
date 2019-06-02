#include <stdio.h>
#include <stdlib.h>
#include <sys/syscall.h>
#include <string.h>

struct _FILE
{
    int fd;

    char* read_buf;
    int available;
    int offset;
    int read_from;
    int ungetc;
    int eof;
    int bufsiz;
    long last_read_start;

    char* _name;
};

FILE _stdin =
{
    .fd = 0,
    .read_buf = NULL,
    .available = 0,
    .offset = 0,
    .read_from = 0,
    .ungetc = -1,
    .eof = 0,
    .last_read_start = 0,
    .bufsiz = BUFSIZ,
};

FILE _stdout =
{
	.fd = 1,
	.read_buf = NULL,
	.available = 0,
	.offset = 0,
	.read_from = 0,
	.ungetc = -1,
	.eof = 0,
	.last_read_start = 0,
	.bufsiz = BUFSIZ,
};

FILE _stderr =
{
	.fd = 2,
	.read_buf = NULL,
	.available = 0,
	.offset = 0,
	.read_from = 0,
	.ungetc = -1,
	.eof = 0,
	.last_read_start = 0,
	.bufsiz = BUFSIZ,
};

FILE* stdin = &_stdin;
FILE* stdout = &_stdout;
FILE* stderr = &_stderr;

void __stdio_init_buffers( void )
{
    _stdin.read_buf = malloc(BUFSIZ);
    _stdin._name = strdup("stdin");
    _stdout._name = strdup("stdout");
    _stderr._name = strdup("stderr");
}
