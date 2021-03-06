#include <sys/stat.h>
#include <sys/syscall.h>
#include <errno.h>
#include <string.h>

DEFN_SYSCALL2(stat, SYS_STATF, char*, void*)
DEFN_SYSCALL2(lstat, SYS_LSTAT, char*, void*)

int stat( const char* file, struct stat* st )
{
    int ret = syscall_stat((char*)file, (void*)st);

    if( ret >= 0 )
    {
        return ret;
    }else
    {
        errno = -ret;
        memset(st, 0, sizeof(struct stat));
        return -1;
    }
}

int lstat( const char* path, struct stat* st )
{
    int ret = syscall_lstat((char*)path, (void*)st);

    if( ret >= 0 )
    {
        return ret;
    }else
    {
        errno = -ret;
        memset(st, 0, sizeof(struct stat));
        return -1;
    }
}
