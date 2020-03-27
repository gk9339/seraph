#include <stdio.h>
#include <stdarg.h>

int sprintf( char* restrict s, const char* restrict fmt, ... )
{
    int ret; 
    va_list args;
    va_start(args, fmt);

    ret = vsprintf(s, fmt, args);

    va_end(args);
    return ret;
}
