#ifndef _DIRENT_H
#define _DIRENT_H

#include <sys/types.h>

typedef struct dirent
{
    uint32_t d_ino;
    char d_name[256];
} dirent;

typedef struct DIR
{
    int fd;
    int cur_entry;
} DIR;

DIR* opendir( const char* name );
int closedir( DIR* dir );
struct dirent* readdir( DIR* dir );

#endif
