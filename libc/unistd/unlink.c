#include <unistd.h>
#include <errno.h>
#include <sys/syscall.h>

DEFN_SYSCALL1(unlink, SYS_UNLINK, char*)

int unlink( const char* path )
{
    __sets_errno(syscall_unlink((char*)path));
}
