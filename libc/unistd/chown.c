#include <unistd.h>
#include <sys/syscall.h>
#include <errno.h>

DEFN_SYSCALL3(chown, SYS_CHOWN, char*, int, int)

int chown( const char* pathname, uid_t owner, gid_t group )
{
    __sets_errno(syscall_chown((char*)pathname, owner, group));
}
