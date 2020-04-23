#ifndef _KERNEL_TMPFS_H
#define _KERNEL_TMPFS_H

#include <stdlib.h>
#include <list.h>
#include <kernel/fs.h>

struct tmpfs_file
{
    char* name;
    int    type;
    int    mask;
    int    uid;
    int    gid;
    unsigned int atime;
    unsigned int mtime;
    unsigned int ctime;
    size_t length;
    size_t block_count;
    size_t pointers;
    char** blocks;
    char* target;
};

struct tmpfs_dir;

struct tmpfs_dir
{
    char* name;
    int    type;
    int    mask;
    int    uid;
    int    gid;
    unsigned int atime;
    unsigned int mtime;
    unsigned int ctime;
    list_t* files;
    struct tmpfs_dir* parent;
};

fs_node_t* tmpfs_create( char* name );
int tmpfs_initialize( void );

#endif
