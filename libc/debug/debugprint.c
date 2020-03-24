#include <debug.h>
#include <sys/syscall.h>
#include <stdarg.h>
#include <stdio.h>

DEFN_SYSCALL1(debugprint, SYS_DEBUGPRINT, char*)

int debugprint( char* message, const char* format, ... )
{
    va_list parameters;
    va_start(parameters, format);
    vsprintf(message, format, parameters);
    va_end(parameters);

    return(syscall_debugprint(message));
}
