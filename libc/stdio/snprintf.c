#include <stdio.h>
#include <stdarg.h>

int snprintf( char* restrict s, size_t n, const char* restrict fmt, ... )
{
    int ret;
    va_list args;
    va_start(args, fmt);

    ret = vsnprintf(s, n, fmt, args);

    va_end(args);
    return ret;
}
