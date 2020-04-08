#include <sys/syscall.h>

int main( void )
{
    syscall_reboot(0);
    __builtin_unreachable();
}
