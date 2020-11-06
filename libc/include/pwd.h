#ifndef PWD_H
#define PWD_H

#include <stdio.h>
#include <sys/types.h>

struct passwd
{
    char* pw_name;
    char* pw_passwd;
    uid_t pw_uid;
    gid_t pw_gid;
    char* pw_comment;
    char* pw_dir;
    char* pw_shell;
};

struct passwd* fgetpwent( FILE* stream );

struct passwd* getpwent( void );
void setpwent( void );
void endpwent( void );

struct passwd* getpwuid( uid_t uid );
struct passwd* getpwnam( const char* name );

#endif
