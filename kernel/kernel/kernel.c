#include <stdio.h>

#include <kernel/kernel.h>
#include <kernel/tty.h>
#include <kernel/gdt.h>
#include <kernel/idt.h>
#include <kernel/isr.h>

void kernel_main(void) 
{
    /* CPU Initialization */
    gdt_initialize();
    idt_initialize();
    isr_initialize();

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

void kpanic( char* error_message, const char* file, int line, struct regs* regs )
{
    printf("PANIC: %s ", error_message);
    printf("File: %s ", file);
    printf("Line: %s ", line);
    if(regs)
    {
        printf("\nREGISTERS:");
        printf("eax=0x%d ebx=0x%d\n", regs->eax, regs->ebx);
        printf("ecx=0x%d edx=0x%d\n", regs->ecx, regs->edx);
        printf("esp=0x%d ebp=0x%d\n", regs->esp, regs->ebp);
        printf("ERRCD: 0x%d", regs->err_code);
        printf("EFLAGSL 0x%d", regs->eflags);
        printf("User ESP: 0x%d", regs->useresp);
        printf("eip=0x%d", regs->eip);
    }
}
