#include <stdio.h>
#include <string.h>
#include <errno.h>
#include <stdint.h>
#include "file.h"

#define MIN(a, b) ((a) < (b) ? (a) : (b))

int vsnprintf( char* str, size_t size, const char* fmt, va_list args )
{
    int retval;
#ifdef __is_libk
    FILE out = 
    {
        .fd = -1,
        .read_base = NULL,
        .read_ptr = NULL,
        .read_end = NULL,
        .available = -1,
        .write_base = str,
        .write_ptr = str,
        .write_end = str + size,
        .bufmode = _IOFBF,
        .ungetc = -1,
        .eof = 0,
        ._name = NULL,
    };
    FILE* file = &out;
#else
    FILE* file = fmemopen(str, size, "w");
#endif
    retval = vfprintf(file, fmt, args);
    if( retval > 0 )
    {
        str[retval] = '\0';
    }
    return retval;
}
