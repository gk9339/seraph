#include <stdlib.h>
#include <unistd.h>

extern void __cxa_finalize( void* );

void exit( int val )
{
    __cxa_finalize(NULL);
    _exit(val);
}
