#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>
#include "ssp.h"
#ifdef __is_libk
#include <kernel/kernel.h>
#include <kernel/random.h>
#endif

uintptr_t __stack_chk_guard = 0x83e8d8bb;

static void __attribute__((constructor)) __guard_setup( void )
{
    unsigned char* p;
#ifdef __is_libk
    __stack_chk_guard = xorshift();
#else
    int fd = open("/dev/random", O_RDONLY);
    if( fd > 0 )
    {
        ssize_t size = read(fd, &__stack_chk_guard, sizeof(__stack_chk_guard));
        close(fd);
        if( size == sizeof(__stack_chk_guard) && __stack_chk_guard != 0 )
            return;
    }
#endif
    p = (unsigned char*) &__stack_chk_guard;
    p[sizeof(__stack_chk_guard)-1] = 255;
    p[sizeof(__stack_chk_guard)-2] = '\n';
    p[0] = 0;
}

void __attribute__((__noreturn__)) __stack_chk_fail( void )
{
#ifdef __is_libk
    KPANIC("Stack smashing detected", NULL)
    STOP    STOP
#else
    fprintf(stderr, "Stack smashing detected. aborting.\n");
    abort();
#endif
}
