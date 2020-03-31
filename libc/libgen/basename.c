#include <libgen.h>
#include <string.h>

char* basename( char* path )
{
    char* p = path;
    char* f = NULL;

    do{
        while( *p == '/' )
        {
            *p = '\0';
            p++;
        }
        if( !*p )
        {
            break;
        }
        f = p;
        p = strchr(f, '/');
    }while( p );

    if( !f )
    {
        return "/";
    }

    return f;
}
