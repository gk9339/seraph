#include <stdio.h>
#include <string.h>
#include <errno.h>
#include <stdint.h>
#include "file.h"

#define MIN(a, b) ((a) < (b) ? (a) : (b))

int vsnprintf( char* str, size_t size, const char* fmt, va_list args )
{
    FILE* file = fmemopen(str, MIN(size, strlen(str)), "w");
    return vfprintf(file, fmt, args);
}
