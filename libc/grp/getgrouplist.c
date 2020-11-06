#include <grp.h>
#include <limits.h>
#include <string.h>

int getgrouplist( const char* user, gid_t group, gid_t* groups, int* ngroups )
{
    ssize_t n, i;
    struct group* g;

    if( *ngroups < 1 )
    {
        return -1;
    }

    n = *ngroups;

    *groups = group;
    *ngroups = 0;

    setgrent();

    while( (g = getgrent()) && *ngroups < INT_MAX )
    {
        for( i = 0; g->gr_mem[i] && strcmp(user, g->gr_mem[i]); i++ );
        if( !g->gr_mem[i] )
        {
            continue;
        }
        if( ++*ngroups <= n )
        {
            *groups++ = g->gr_gid;
        }
    }

    endgrent();

    return *ngroups > n ? -1 : *ngroups;
}
