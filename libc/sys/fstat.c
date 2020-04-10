#include <unistd.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <errno.h>

DEFN_SYSCALL2(fstat, SYS_STAT, int, void*)

int fstat( int filedes, struct stat* st )
{
    __sets_errno(syscall_fstat(filedes, st));
}
