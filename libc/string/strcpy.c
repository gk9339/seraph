#include <stddef.h>
#include <stdlib.h>
#include <limits.h>
#include <kernel/types.h>

#define ONES ((size_t)-1/UCHAR_MAX)
#define HIGHS (ONES * (UCHAR_MAX/2+1))
#define HASZERO(X) (((X)-ONES) & ~(X) & HIGHS)

/* copy a string */
char* strcpy( char* restrict dest, const char* restrict src )
{
    size_t* wd;
    const size_t* ws;

    if( (uintptr_t)src % sizeof(size_t) == (uintptr_t)dest % sizeof(size_t) )
    {
        for(; (uintptr_t)src % sizeof(size_t); src++, dest++ )
        {
            if( !(*dest=*src) )
            {
                return dest;
            }
        }

        wd = (void*)dest;
        ws = (const void*)src;
        for(; !HASZERO(*ws); *wd++ = *ws++ );
        dest = (void*)wd;
        src = (const void*)ws;
    }

    for(; (*dest=*src); src++, dest++);

    return dest;
}
