#include <string.h>
#include <sys/types.h>

char* strcat( char* dest, const char* src )
{
    char* end = dest;
	while( *end != '\0' )
    {
		++end;
	}
	while( *src )
    {
		*end = *src;
		end++;
		src++;
	}
	*end = '\0';
	return dest;
}

char* strncat( char* dest, const char* src, size_t n )
{
	char* end = dest;
	while( *end != '\0' )
    {
		++end;
	}
	size_t i = 0;
	while( *src && i < n )
    {
		*end = *src;
		end++;
		src++;
		i++;
	}
	*end = '\0';
	return dest;
}
