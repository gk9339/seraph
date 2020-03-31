#include <libgen.h>
#include <string.h>

char* dirname( char* path )
{
    int has_slash = 0;
    char* p = path;

    while( *p )
    {
        if( *p == '/' )
        {
            has_slash = 1;
        }
        p++;
    }
    if( !has_slash )
    {
        return ".";
    }

    p--;
    while( *p == '/' )
    {
        *p = '\0';
        if( p == path )
        {
            break;
        }
        p--;
    }

    if( p == path )
    {
        return "/";
    }

    while( *p != '/' )
    {
        *p = '\0';
        if( p == path )
        {
            break;
        }
        p--;
    }

    if( p == path )
    {
        if( *p == '/' )
        {
            return "/";
        }
        return ".";
    }

    while( *p == '/' )
    {
        if( p == path )
        {
            return "/";
        }
        *p = '\0';
        p--;
    }

    return path;
}
