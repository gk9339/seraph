#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include "ssp.h"
#ifdef __is_libk
#include <kernel/kernel.h>
#endif

uintptr_t __stack_chk_guard = 0x83e8d8bb;

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
