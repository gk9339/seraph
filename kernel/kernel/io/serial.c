#include <kernel/serial.h>
#include <stdio.h>

void debug_logf( char* str, const char* format, ... )
{
    va_list parameters;
    va_start(parameters, format);
    vsprintf(str, format, parameters);
    va_end(parameters);

    debug_log(str);
}
