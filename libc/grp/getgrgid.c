#include <grp.h>

struct group* getgrgid( gid_t gid )
{
    struct group* g;

    setgrent();

    while( (g = getgrent()) )
    {
        if( g->gr_gid == gid )
        {
            return g;
        }
    }

    return NULL;
}
