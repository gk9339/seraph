#include <errno.h>
#include <sys/syscall.h>
#include <sys/stat.h>

DEFN_SYSCALL2(mkdir, SYS_MKDIR, char*, unsigned int)

int mkdir( const char* pathname, mode_t mode )
{
    __sets_errno(syscall_mkdir((char*)pathname, mode));
}
