#include <unistd.h>
#include <sys/syscall.h>
#include <errno.h>

DEFN_SYSCALL1(chdir, SYS_CHDIR, char*)

int chdir( const char* path )
{
    __sets_errno(syscall_chdir((char*)path));
}
