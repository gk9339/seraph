#include <grp.h>
#include <stdlib.h>

static FILE* grdb = NULL;
static struct group* gr_ent;
static char* gr_blob;

struct group* getgrent( void )
{
    if( !grdb )
    {
        grdb = fopen("/conf/group", "r");
    }

    if( !grdb )
    {
        return NULL;
    }

    return fgetgrent(grdb);
}

void setgrent( void )
{
    if( grdb )
    {
        rewind(grdb);
    }
}

void endgrent( void )
{
    if( grdb )
    {
        fclose(grdb);
        grdb = NULL;
    }

    if( gr_ent )
    {
        free(gr_ent);
        free(gr_blob);

        gr_ent = NULL;
        gr_blob = NULL;
    }
}
