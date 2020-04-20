#ifndef _UTSNAME_H
#define _USTNAME_H

#ifdef __cplusplus
extern "C" {
#endif

#define _UTSNAME_LENGTH 128

struct utsname
{
    char sysname[_UTSNAME_LENGTH];
    char nodename[_UTSNAME_LENGTH];
    char release[_UTSNAME_LENGTH];
    char version[_UTSNAME_LENGTH];
    char machine[_UTSNAME_LENGTH];
    char domainname[_UTSNAME_LENGTH];
};

int uname( struct utsname* __name );

#ifdef __cplusplus
}
#endif

#endif
