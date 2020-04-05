#include <sys/utsname.h>
#include <sys/syscall.h>

DEFN_SYSCALL1(uname, SYS_UNAME, void*)

int uname( struct utsname* __name )
{
    return syscall_uname((void*)__name);
}
