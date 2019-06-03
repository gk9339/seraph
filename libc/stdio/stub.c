#include <stdio.h>

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

int fopen( const char* pathname, const char* mode )
{
    return 0;
}

int fclose( FILE* stream )
{
    return 0;
}

size_t fread( void* ptr, size_t size, size_t nmemb, FILE* stream )
{
    return 0;
}

size_t fwrite( const void* ptr, size_t size, size_t nmemb, FILE* stream )
{
    return 0;
}

int fseek( FILE* stream, long offset, int whence )
{
    return 0;
}

long ftell( FILE* stream )
{
    return 0;
}

void setbuf( FILE* stream, char* buf )
{

}

int fflush( FILE* stream )
{
    return 0;
}

int fprintf( FILE* file, const char* format, ... )
{
    return 0;
}

int vprintf( FILE* file, const char* format, va_list args )
{
    return 0;
}
