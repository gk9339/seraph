#include <stdio.h>

#if defined(__is_libk)
#include <kernel/tty.h>
#endif

int putchar( int ic ) 
{
	char c = (char)ic;
	terminal_write(&c, sizeof(c));
	
    return ic;
}
