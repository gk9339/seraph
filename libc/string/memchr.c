#include <string.h>
#include <sys/types.h>
#include <limits.h>

#define ALIGN (sizeof(size_t))
#define ONES ((size_t)-1/UCHAR_MAX)
#define HIGHS (ONES * (UCHAR_MAX/2+1))
#define HASZERO(X) (((X)-ONES) & ~(X) & HIGHS)

/* scan memory for a character */
void* memchr( const void* s, int c, size_t n )
{
	const unsigned char* str = s;
	unsigned char chr = (unsigned char)c;

	for(; ((uintptr_t)str & (ALIGN - 1)) && n && *str != chr; str++, n-- );
	
    if( n && *str != chr )
    {
		const size_t * w;
		size_t k = ONES * chr;

		for( w = (const void*)str; n >= sizeof(size_t) && !HASZERO(*w^k); w++, n -= sizeof(size_t) );
		for( str = (const void*)w; n && *str != chr; str++, n-- );
	}

	return n ? (void*)s : 0;
}
