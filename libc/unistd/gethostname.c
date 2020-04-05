#include <unistd.h>
#include <sys/syscall.h>
#include <errno.h>

DEFN_SYSCALL2(gethostname, SYS_GETHOSTNAME, char*, size_t)

int gethostname( char* name, size_t len )
{
    __sets_errno(syscall_gethostname(name, len));
}
