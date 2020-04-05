#include <unistd.h>
#include <sys/syscall.h>
#include <errno.h>

DEFN_SYSCALL2(sethostname, SYS_SETHOSTNAME, char*, size_t)

int sethostname( const char* name, size_t len )
{
    __sets_errno(syscall_sethostname((char*)name, len));
}
