#include <unistd.h>
#include <sys/syscall.h>

int sleep( unsigned int seconds )
{
    syscall_nanosleep(seconds, 0);
    
    return 0;
}
