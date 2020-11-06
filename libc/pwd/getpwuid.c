#include <pwd.h>

struct passwd* getpwuid( uid_t uid )
{
    struct passwd* p;

    setpwent();

    while( (p = getpwent()) )
    {
        if( p->pw_uid == uid )
        {
            return p;
        }
    }

    return NULL;
}
