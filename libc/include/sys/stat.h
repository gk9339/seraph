#ifndef _STAT_H
#define _STAT_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stdint.h>
#include <sys/types.h>
#include <time.h>

#define S_IREAD     0000400    /* read permission, owner */
#define S_IRUSR     S_IREAD
#define S_IWRITE    0000200    /* write permission, owner */
#define S_IWUSR     S_IWRITE
#define S_IEXEC     0000100    /* execute/search permission, owner */
#define S_IXUSR     S_IEXEC
#define S_IRWXU     (S_IRUSR | S_IWUSR | S_IXUSR)
#define S_IRGRP     0000040    /* read permission, group */
#define S_IWGRP     0000020    /* write permission, group */
#define S_IXGRP     0000010    /* execute/search permission, group */
#define S_IRWXG     (S_IRGRP | S_IWGRP | S_IXGRP)
#define S_IROTH     0000004    /* read permission, others */
#define S_IWOTH     0000002    /* write permission, others */
#define S_IXOTH     0000001    /* execute/search permission, others */
#define S_IRWXO     (S_IROTH | S_IWOTH | S_IXOTH)
#define S_ISUID     0004000    /* set user id on execution */
#define S_ISGID     0002000    /* set group id on execution */
#define S_ISVTX     0001000    /* save swapped text even after use */
#define S_ENFMT     0002000    /* enforcement-mode locking */

#define _IFMT       0170000 /* type of file */
#define     _IFDIR  0040000 /* directory */
#define     _IFCHR  0020000 /* character special */
#define     _IFBLK  0060000 /* block special */
#define     _IFREG  0100000 /* regular */
#define     _IFLNK  0120000 /* symbolic link */
#define     _IFSOCK 0140000 /* socket */
#define     _IFIFO  0010000 /* fifo */

#define S_IFMT  _IFMT
#define S_IFDIR  _IFDIR
#define S_IFCHR  _IFCHR
#define S_IFBLK  _IFBLK
#define S_IFREG  _IFREG
#define S_IFLNK  _IFLNK
#define S_IFSOCK _IFSOCK
#define S_IFIFO  _IFIFO

#define S_ISBLK(m) (((m)&_IFMT) == _IFBLK)
#define S_ISCHR(m) (((m)&_IFMT) == _IFCHR)
#define S_ISDIR(m) (((m)&_IFMT) == _IFDIR)
#define S_ISFIFO(m) (((m)&_IFMT) == _IFIFO)
#define S_ISREG(m) (((m)&_IFMT) == _IFREG)
#define S_ISLNK(m) (((m)&_IFMT) == _IFLNK)
#define S_ISSOCK(m) (((m)&_IFMT) == _IFSOCK)

struct stat
{
    dev_t st_dev; /* Device ID of device containing file */
    ino_t st_ino; /* File serial number */
    mode_t st_mode; /* Mode of file */
    nlink_t st_nlink; /* Number of hard links to file */
    uid_t st_uid; /* User ID of file */
    gid_t st_gid; /* Group ID of file */
    dev_t st_rdev; /* Device ID (if file is character of block special) */
    off_t st_size; /* For regular files, size in bytes, symlink, length in bytes of the pathname in the link,
                         shmem object, length in bytes, typed memory object, length in bytes */
    struct timespec st_atim; /* Time of last access */
    uint32_t __unused1;
    struct timespec st_mtim; /* Time of last modification */
    uint32_t __unused2;
    struct timespec st_ctim; /* Time of last status change */
    uint32_t __unused3;
#define st_atime st_atim.tv_sec
#define st_mtime st_mtim.tv_sec
#define st_ctime st_ctim.tv_sec
    uint32_t st_blksize; /* Filesystem specific perferred I/O block size of the object */
    uint32_t st_blocks; /* Number of blocks allocated to this object */
};

int stat( const char* file, struct stat* st );
int lstat( const char* path, struct stat* st );
int fstat( int file, struct stat* st );

int mkdir( const char* pathname, mode_t mode );
mode_t umask( mode_t mask );

#ifdef __cplusplus
}
#endif

#endif
