#include <stdio.h>

#include <kernel/tty.h>

void kernel_main(void) 
{
	char str[1];
    /* Initialize terminal interface */
	terminal_initialize();
 
    printf("All char test:\n");

    for( int i = 32; i <= 255; i++ )
    {
        str[0] = i;
            printf("%s",str);
    }

    printf("\nBackspace test:\n");

    printf("fail\b\b\b\bpass\n");
}
