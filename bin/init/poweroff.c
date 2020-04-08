#include <sys/syscall.h>

int main( void )
{
    syscall_reboot(1);
    __builtin_unreachable();
}
