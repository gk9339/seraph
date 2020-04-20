#include <stdio.h>
#include <stdarg.h>

int scanf( const char* format, ... )
{
    va_list args;
    va_start(args, format);

    int out = vfscanf(stdin, format, args);
    
    va_end( args );
    return out;
}
