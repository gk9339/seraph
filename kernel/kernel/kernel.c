#include <stdio.h>

#include <kernel/tty.h>
#include <kernel/gdt.h>
#include <kernel/idt.h>

void kernel_main(void) 
{
    /* Initialize GDT */
    gdt_initialize();

    /* Initialize terminal interface */
	terminal_initialize();
 
    printf("All char test:\n");

	char str[1];
    for( int i = 32; i <= 255; i++ )
    {
        str[0] = i;
            printf("%s",str);
    }

    printf("\nBackspace test:\n");

    printf("fail\b\b\b\bpass\n");
}
