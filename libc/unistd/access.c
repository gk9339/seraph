#include <unistd.h>
#include <sys/syscall.h>
#include <errno.h>

DEFN_SYSCALL2(access, SYS_ACCESS, char*, int)

int access( const char* pathname, int mode )
{
    int retval = syscall_access((char*)pathname, mode);

    if( retval < 0 )
    {
        errno = ENOENT;
        return -1;
    }

    return retval;
}
