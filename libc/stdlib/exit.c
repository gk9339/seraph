#include <stdlib.h>
#include <unistd.h>

extern void _exit( int );
extern void _handle_atexit( void );

void exit( int val )
{
    _handle_atexit();
    _exit(val);
}
