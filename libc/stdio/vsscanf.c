#include <stdio.h>
#include <stdarg.h>
#include <string.h>
#include <stdlib.h>

int vsscanf( const char* str, const char* format, va_list args )
{
    int retval;

    FILE* file = fmemopen((void*)str, strlen(str), "w");
    retval = vfscanf(file, format, args);

    free(file);
    return retval;
}
