#include <string.h>
#include <sys/types.h>

#define BITOP(A, B, OP) ((A)[(size_t)(B)/(8*sizeof *(A))] OP (size_t)1<<((size_t)(B)%(8*sizeof *(A))))
#define MAX(A, B) ((A) > (B) ? (A) : (B))

static char* strstr_2b( const unsigned char* haystack, const unsigned char* needle )
{
	uint16_t nw = (uint16_t)(needle[0] << 8 | needle[1]);
	uint16_t hw = (uint16_t)(haystack[0] << 8 | haystack[1]);
	for( haystack++; *haystack && hw != nw; hw = (uint16_t)(hw << 8 | *haystack++) );
	return *haystack ? (char *)haystack-1 : 0;
}

static char* strstr_3b( const unsigned char* haystack, const unsigned char* needle )
{
	uint32_t nw = needle[0] << 24 | needle[1] << 16 | needle[2] << 8;
	uint32_t hw = haystack[0] << 24 | haystack[1] << 16 | haystack[2] << 8;
	for( haystack += 2; *haystack && hw != nw; hw = (hw|*haystack++) << 8 );
	return *haystack ? (char *)haystack-2 : 0;
}

static char* strstr_4b( const unsigned char* haystack, const unsigned char* needle )
{
	uint32_t nw = needle[0] << 24 | needle[1] << 16 | needle[2] << 8 | needle[3];
	uint32_t hw = haystack[0] << 24 | haystack[1] << 16 | haystack[2] << 8 | haystack[3];
	for( haystack += 3; *haystack && hw != nw; hw = hw << 8 | *haystack++ );
	return *haystack ? (char *)haystack-3 : 0;
}

static char* strstr_twoway( const unsigned char* haystack, const unsigned char* needle )
{
	size_t mem;
	size_t mem0;
	size_t byteset[32 / sizeof(size_t)] = { 0 };
	size_t shift[256];
	size_t l;

	/* Computing length of needle and fill shift table */
	for( l = 0; needle[l] && haystack[l]; l++ )
    {
		BITOP(byteset, needle[l], |=);
		shift[needle[l]] = l+1;
	}

	if( needle[l] )
    {
		return 0; /* hit the end of haystack */
	}

	/* Compute maximal suffix */
	size_t ip = -1;
	size_t jp = 0;
	size_t k = 1;
	size_t p = 1;
	while( jp+k<l )
    {
		if( needle[ip+k] == needle[jp+k] )
        {
			if( k == p )
            {
				jp += p;
				k = 1;
			}else
            {
				k++;
			}
		}else if( needle[ip+k] > needle[jp+k] )
        {
			jp += k;
			k = 1;
			p = jp - ip;
		}else
        {
			ip = jp++;
			k = p = 1;
		}
	}
	size_t ms = ip;
	size_t p0 = p;

	/* And with the opposite comparison */
	ip = -1;
	jp = 0;
	k = p = 1;
	while( jp+k<l )
    {
		if( needle[ip+k] == needle[jp+k] )
        {
			if( k == p )
            {
				jp += p;
				k = 1;
			}else
            {
				k++;
			}
		}else if( needle[ip+k] < needle[jp+k] )
        {
			jp += k;
			k = 1;
			p = jp - ip;
		}else
        {
			ip = jp++;
			k = p = 1;
		}
	}
	if( ip+1 > ms+1 )
    {
		ms = ip;
	}else
    {
		p = p0;
	}

	/* Periodic needle? */
	if( memcmp(needle, needle+p, ms+1) )
    {
		mem0 = 0;
		p = MAX(ms, l-ms-1) + 1;
	}else
    {
		mem0 = l-p;
	}
	mem = 0;

	/* Initialize incremental end-of-haystack pointer */
	const unsigned char * z = haystack;

	/* Search loop */
	for(;; )
    {
		/* Update incremental end-of-haystack pointer */
		if( (size_t)(z-haystack) < l )
        {
			/* Fast estimate for MIN(l,63) */
			size_t grow = l | 63;
			const unsigned char *z2 = memchr(z, 0, grow);
			if( z2 )
            {
				z = z2;
				if( (size_t)(z-haystack) < l )
                {
					return 0;
				}
			}else
            {
				z += grow;
			}
		}

		/* Check last byte first; advance by shift on mismatch */
		if( BITOP(byteset, haystack[l-1], &) )
        {
			k = l-shift[haystack[l-1]];
			if( k )
            {
				if( mem0 && mem && k < p ) k = l-p;
				haystack += k;
				mem = 0;
				continue;
			}
		}else
        {
			haystack += l;
			mem = 0;
			continue;
		}

		/* Compare right half */
		for( k=MAX(ms+1,mem); needle[k] && needle[k] == haystack[k]; k++ );
		if( needle[k] )
        {
			haystack += k-ms;
			mem = 0;
			continue;
		}
		/* Compare left half */
		for( k=ms+1; k>mem && needle[k-1] == haystack[k-1]; k-- );
		if( k <= mem )
        {
			return (char*)haystack;
		}
		haystack += p;
		mem = mem0;
	}
}

/* locate a substring */
char* strstr( const char* haystack, const char* needle )
{
	/* Return immediately on empty needle */
	if( !needle[0] )
    {
		return (char*)haystack;
	}

	/* Use faster algorithms for short needles */
	haystack = strchr(haystack, *needle);
	if( !haystack || !needle[1] )
    {
		return (char*)haystack;
	}

	if( !haystack[1] )return 0;
	if( !needle[2] )return strstr_2b((void*)haystack, (void*)needle);
	if( !haystack[2] )return 0;
	if( !needle[3] )return strstr_3b((void*)haystack, (void*)needle);
	if( !haystack[3] )return 0;
	if( !needle[4] )return strstr_4b((void*)haystack, (void*)needle);

	/* Two-way on large needles */
	return strstr_twoway((void*)haystack, (void*)needle);
}
