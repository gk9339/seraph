#include <stdio.h>
#include <stdint.h>

int sscanf( const char* str, const char* format, ... )
{
    va_list args;
    va_start(args, format);

    int out = vsscanf(str, format, args);
    
    va_end( args );
    return out;
}
