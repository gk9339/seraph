#include <stdio.h>

#include <kernel/tty.h>

void kernel_main(void) 
{
	char str[1];
    /* Initialize terminal interface */
	terminal_initialize();
 
    for( int i = 1; i <= 255; i++ )
    {
        str[0] = i;
        if( str[0] != '\n' )
            terminal_writestring(str);
    }
}
