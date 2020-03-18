#ifndef _STAT_H
#define _STAT_H

#include <sys/types.h>

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
    uint16_t st_dev; /* Device ID of device containing file */
    uint16_t st_ino; /* File serial number */
    uint32_t st_mode; /* Mode of file */
    uint16_t st_nlink; /* Number of hard links to file */
    uint16_t st_uid; /* User ID of file */
    uint16_t st_gid; /* Group ID of file */
    uint16_t st_rdev; /* Device ID (if file is character of block special) */
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

#endif
