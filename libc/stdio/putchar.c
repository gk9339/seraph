#include <stdio.h>

#ifdef __is_libk
#include <kernel/vga.h>
#endif

int putchar( int ic ) 
{
	char c = (char)ic;
#ifdef __is_libk
	terminal_write(&c, sizeof(c));
    return 0;
#else
    return fwrite(&c, sizeof(char), 1, stdout);
#endif
}
