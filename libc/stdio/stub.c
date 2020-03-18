#include <stdio.h>

#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wunused-parameter"

int fclose( FILE* stream )
{
    return 0;
}

size_t fread( void* ptr, size_t size, size_t nmemb, FILE* stream )
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

int vfprintf( FILE* file, const char* format, va_list args )
{
    return 0;
}

#pragma GCC diagnostic pop
