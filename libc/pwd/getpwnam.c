#include <pwd.h>
#include <string.h>

struct passwd* getpwnam( const char* name )
{
    struct passwd* p;

    setpwent();

    while( (p = getpwent()) )
    {
        if( !strcmp(p->pw_name, name) )
        {
            return p;
        }
    }

    return NULL;
}
