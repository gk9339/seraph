#include <string.h>

/* calculate the length of a string */
size_t strlen( const char* str ) 
{
	size_t len = 0;

    while( str[len] )
		len++;
	
    return len;
}
