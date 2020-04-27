#include <sys/mount.h>
#include <sys/syscall.h>
#include <errno.h>

DEFN_SYSCALL3(mount, SYS_MOUNT, char*, char*, char*)

int mount( char* source, char* target, char* type )
{
    __sets_errno(syscall_mount(source, target, type));
}
