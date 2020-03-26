#include <dirent.h>
#include <sys/syscall.h>
#include <string.h>
#include <unistd.h>
#include <stdlib.h>
#include <errno.h>
#include <fcntl.h>

DEFN_SYSCALL3(readdir, SYS_READDIR, int, int, void*)

DIR* opendir( const char* name )
{
    int fd = open(name, O_RDONLY);
    if( fd < 0 )
    {
        return NULL;
    }

    DIR* dir = (DIR*)malloc(sizeof(DIR));
    dir->fd = fd;
    dir->cur_entry = -1;

    return dir;
}

int closedir( DIR* dir )
{
    if( dir && (dir->fd != -1) )
    {
        return close(dir->fd);
    }else
    {
        return -EBADF;
    }
}

struct dirent* readdir( DIR* dir )
{
    static struct dirent ent;

    int ret = syscall_readdir(dir->fd, ++dir->cur_entry, &ent);
    if( ret < 0 )
    {
        errno = -ret;
        memset(&ent, 0, sizeof(struct dirent));
        return NULL;
    }

    if( ret == 0 )
    {
        memset(&ent, 0, sizeof(struct dirent));
        return NULL;
    }

    return &ent;
}
