#include <string.h>
#include <stdint.h>
#include <limits.h>

#define ALIGN (sizeof(size_t))

#define ONES ((size_t)-1/UCHAR_MAX)
#define HIGHS (ONES * (UCHAR_MAX/2+1))
#define HASZERO(X) (((X)-ONES) & ~(X) & HIGHS)

/* locate character in string */
char* strchr( const char* s, int c )
{
    char* r = strchrnul(s, c);
    return *(unsigned char*)r == (unsigned char)c ? r : 0;
}

char* strchrnul( const char* s, int c )
{
    size_t * w;
    size_t k;

    c = (unsigned char)c;
    if( !c )
    {
        return (char *)s + strlen(s);
    }

    for(; (uintptr_t)s % ALIGN; s++ )
    {
        if( !*s || *(unsigned char *)s == c )
        {
            return (char *)s;
        }
    }

    k = ONES * c;
    for(w = (void *)s; !HASZERO(*w) && !HASZERO(*w^k); w++ );
    for(s = (void *)w; *s && *(unsigned char *)s != c; s++ );
    
    return (char *)s;
}

char* strrchr( const char* s, int c )
{
    return memrchr(s, c, strlen(s) + 1);
}
