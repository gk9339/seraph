#include "ssp.h"

void __attribute__((__noreturn__)) __stack_chk_fail_local( void )
{
    __stack_chk_fail();
}
