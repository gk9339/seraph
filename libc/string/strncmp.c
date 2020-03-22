#include <string.h>

int strncmp( const char* x, const char* y, size_t n )
{
    if( n==0 )
    {
        return 0;
    }

    while( n-- && (*x == *y) )
    {
        if( !n || !*x )
        {
            break;
        }

        x++;
        y++;
    }

    return (*(unsigned char*)x) - (*(unsigned char*)y);
}
