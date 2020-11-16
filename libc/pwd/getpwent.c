#include <pwd.h>
#include <stdlib.h>

#ifdef __is_libk
#include <fcntl.h>
#include <kernel/fs.h>
static fs_node_t* pwdb = NULL;
#else
static FILE* pwdb = NULL;
static struct passwd* pw_ent;
static char* pw_blob;
#endif

struct passwd* getpwent( void )
{
    if( !pwdb )
    {
#ifdef __is_libk
        pwdb = kopen("/conf/passwd", O_RDONLY);
#else
        pwdb = fopen("/conf/passwd", "r");
#endif
    }

    if( !pwdb )
    {
        return NULL;
    }

    return fgetpwent((FILE*)pwdb);
}

void setpwent( void )
{
#ifndef __is_libk
    if( pwdb )
    {
        rewind(pwdb);
    }
#endif
}

void endpwent( void )
{
#ifndef __is_libk
    if( pwdb )
    {
        fclose(pwdb);
        pwdb = NULL;
    }

    if( pw_ent )
    {
        free(pw_ent);
        free(pw_blob);

        pw_ent = NULL;
        pw_blob = NULL;
    }
#endif
}
