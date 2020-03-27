#include <stdio.h>
#include <limits.h>

int vsprintf( char* restrict s, const char* restrict fmt, va_list args )
{
    return vsnprintf(s, INT_MAX, fmt, args);
}
