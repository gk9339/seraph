#ifndef _STAT_H
#define _STAT_H

#include <sys/types.h>

#define S_ISUID     0004000    /* set user id on execution */
#define S_ISGID     0002000    /* set group id on execution */
#define S_ISVTX     0001000    /* save swapped text even after use */
#define S_IREAD     0000400    /* read permission, owner */
#define S_IWRITE    0000200    /* write permission, owner */
#define S_IEXEC     0000100    /* execute/search permission, owner */
#define S_ENFMT     0002000    /* enforcement-mode locking */

#define _IFMT       0170000 /* type of file */
#define     _IFDIR  0040000 /* directory */
#define     _IFCHR  0020000 /* character special */
#define     _IFBLK  0060000 /* block special */
#define     _IFREG  0100000 /* regular */
#define     _IFLNK  0120000 /* symbolic link */
#define     _IFSOCK 0140000 /* socket */
#define     _IFIFO  0010000 /* fifo */

struct stat
{
    uint32_t st_dev; /* Device ID of device containing file */
    uint32_t st_ino; /* File serial number */
    uint32_t st_mode; /* Mode of file */
    uint32_t st_nlink; /* Number of hard links to file */
    uint32_t st_uid; /* User ID of file */
    uint32_t st_gid; /* Group ID of file */
    uint32_t st_rdev; /* Device ID (if file is character of block special) */
    uint32_t st_size; /* For regular files, size in bytes, symlink, length in bytes of the pathname in the link,
                         shmem object, length in bytes, typed memory object, length in bytes */
    uint32_t st_atime; /* Time of last access */
    uint32_t __unused1;
    uint32_t st_mtime; /* Time of last modification */
    uint32_t __unused2;
    uint32_t st_ctime; /* Time of last status change */
    uint32_t __unused3;
    uint32_t st_blksize; /* Filesystem specific perferred I/O block size of the object */
    uint32_t st_blocks; /* Number of blocks allocated to this object */
};

int stat( const char* file, struct stat* st );
int lstat( const char* path, struct stat* st );

#endif