#include <pwd.h>
#include <stdlib.h>

static FILE* pwdb = NULL;
static struct passwd* pw_ent;
static char* pw_blob;

struct passwd* getpwent( void )
{
    if( !pwdb )
    {
        pwdb = fopen("/conf/passwd", "r");
    }

    if( !pwdb )
    {
        return NULL;
    }

    return fgetpwent(pwdb);
}

void setpwent( void )
{
    if( pwdb )
    {
        rewind(pwdb);
    }
}

void endpwent( void )
{
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
}
