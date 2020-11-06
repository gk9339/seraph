#include <grp.h>
#include <string.h>

struct group* getgrnam( const char* name )
{
    struct group* g;

    setgrent();

    while( (g = getgrent()) )
    {
        if( !strcmp(g->gr_name, name) )
        {
            return g;
        }
    }

    return NULL;
}
