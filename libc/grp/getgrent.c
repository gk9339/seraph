#include <grp.h>
#include <stdlib.h>

#ifdef __is_libk
#include <fcntl.h>
#include <kernel/fs.h>
static fs_node_t* grdb = NULL;
#else
static FILE* grdb = NULL;
static struct group* gr_ent;
static char* gr_blob;
#endif

struct group* getgrent( void )
{
    if( !grdb )
    {
#ifdef __is_libk
        grdb = kopen("/conf/group", O_RDONLY);
#else
        grdb = fopen("/conf/group", "r");
#endif
    }

    if( !grdb )
    {
        return NULL;
    }

    return fgetgrent((FILE*)grdb);
}

void setgrent( void )
{
#ifndef __is_libk
    if( grdb )
    {
        rewind(grdb);
    }
#endif
}

void endgrent( void )
{
#ifndef __is_libk
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
#endif
}
