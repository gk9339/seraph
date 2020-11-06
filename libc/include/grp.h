#ifndef GRP_H
#define GRP_H

#include <stdio.h>
#include <sys/types.h>

struct group
{
    char* gr_name;
    gid_t gr_gid;
    char** gr_mem;
};

struct group* fgetgrent( FILE* stream );

struct group* getgrent( void );
void setgrent( void );
void endgrent( void );

struct group* getgrgid( gid_t gid );
struct group* getgrnam( const char* gr_name );

int getgrouplist( const char* user, gid_t group, gid_t* groups, int* ngroups );

#endif
