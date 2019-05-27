#include <string.h>
#include <sys/types.h>
#include <limits.h>

#define ALIGN (sizeof(size_t))
#define ONES ((size_t)-1/UCHAR_MAX)
#define HIGHS (ONES * (UCHAR_MAX/2+1))
#define HASZERO(X) (((X)-ONES) & ~(X) & HIGHS)

void* memchr( const void* src, int c, size_t n )
{
	const unsigned char* s = src;
	c = (unsigned char)c;
	for(; ((uintptr_t)s & (ALIGN - 1)) && n && *s != c; s++, n-- );
	if( n && *s != c )
    {
		const size_t * w;
		size_t k = ONES * c;
		for( w = (const void*)s; n >= sizeof(size_t) && !HASZERO(*w^k); w++, n -= sizeof(size_t) );
		for( s = (const void*)w; n && *s != c; s++, n-- );
	}
	return n ? (void*)s : 0;
}
