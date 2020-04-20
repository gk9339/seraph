#include <string.h>

size_t strxfrm( char* dest, const char* src, size_t n )
{
    size_t i = 0;
    
    while( *src && i < n )
    {
        *dest = *src;
        dest++;
        src++;
        i++;
    }

    if( i < n )
    {
        *dest = '\0';
    }

    return i;
}
