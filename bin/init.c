#include <sys/syscall.h>
#include <unistd.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <kernel/tty.h>

int main( int argc, char** argv )
{
    printf("hi??");

    syscall_open("/dev/null", 0, 0);
    syscall_open("/dev/null", 1, 0);
    syscall_open("/dev/null", 1, 0);

    __asm__("cli"); // General Protection Fault
    
    return 1;
}
