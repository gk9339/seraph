#include <string.h>

/* compare two strings */
int strcmp( const char* x, const char* y )
{
    for(; *x == *y && *x; x++, y++ );
 
    return *(unsigned char *)x - *(unsigned char*)y;
}
