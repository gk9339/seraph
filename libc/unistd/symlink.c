#include <unistd.h>
#include <errno.h>
#include <sys/syscall.h>

DEFN_SYSCALL2(symlink, SYS_SYMLINK, char*, char*)

int symlink( const char* target, const char* linkpath )
{
    __sets_errno(syscall_symlink((char*)target, (char*)linkpath));
}
