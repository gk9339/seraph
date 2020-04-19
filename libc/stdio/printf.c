#include <stdio.h>
#include <stdarg.h>

#ifdef __is_libk
#include <stdlib.h>
#include <kernel/lfb.h>
#endif

int printf( const char* restrict fmt, ... )
{
    int ret;
    va_list args;
    va_start(args, fmt);
    
#ifdef __is_libk
    char kstr[512];
    ret = vsprintf(kstr, fmt, args);
    terminal_writestring(kstr);
#else
    ret = vfprintf(stdout, fmt, args);
#endif

    va_end(args);
    return ret;
}
