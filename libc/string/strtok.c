#include <string.h>

#define BITOP(A, B, OP) ((A)[(size_t)(B)/(8*sizeof *(A))] OP (size_t)1<<((size_t)(B)%(8*sizeof *(A))))

char* strtok( char* str, const char* delim )
{
    static char* saveptr = NULL;
    return strtok_r(str, delim, &saveptr);
}

char* strtok_r( char* str, const char* delim, char** saveptr )
{
    char* token;

    if( str == NULL )
    {
        str = *saveptr;
    }

    str += strspn(str, delim);
    if( *str == '\0' )
    {
        *saveptr = str;
        return NULL;
    }

    token = str;
    str = strpbrk(token, delim);
    if( str == NULL )
    {
        *saveptr = (char*)lfind(token, '\0');
    }else
    {
        *str = '\0';
        *saveptr = str + 1;
    }

    return token;
}

size_t strspn( const char* s, const char* c )
{
    const char * a = s;
	size_t byteset[32/sizeof(size_t)] = { 0 };

	if( !c[0] )
    {
		return 0;
	}
	if( !c[1] )
    {
		for(; *s == *c; s++ );
		return s-a;
	}

	for(; *c && BITOP(byteset, *(unsigned char *)c, |=); c++ );
	for(; *s && BITOP(byteset, *(unsigned char *)s, &); s++ );

    return s-a;
}

size_t strcspn( const char* s, const char* c )
{
    const char *a = s;
	if( c[0] && c[1] )
    {
		size_t byteset[32/sizeof(size_t)] = { 0 };
		for(; *c && BITOP(byteset, *(unsigned char *)c, |=); c++ );
		for(; *s && !BITOP(byteset, *(unsigned char *)s, &); s++ );
		return s-a;
	}
    return strchrnul(s, *c)-a;
}

size_t lfind( const char* str, const char accept )
{
	return (size_t)strchr(str, accept);
}

char* strpbrk( const char* s, const char* b )
{
    s += strcspn(s, b);
    return *s ? (char *)s : 0;
}
