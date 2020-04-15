#include "ssp.h"

void __attribute__((__noreturn__)) __attribute__((visibility("hidden"))) __stack_chk_fail_local( void )
{
    __stack_chk_fail();
}
