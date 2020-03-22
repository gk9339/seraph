#include <stdio.h>

#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wunused-parameter"

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
