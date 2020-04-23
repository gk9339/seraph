#include <stdio.h>
#include "file.h"

char* fgets( char* s, int size, FILE* stream )
{
	int c;
	char* out = s;

	while( (c = fgetc(stream)) > 0 )
    {
		*s++ = c;
		size--;
		if( size == 0 )
        {
			return out;
		}
		*s = '\0';
		if( c == '\n' )
        {
			return out;
		}
	}

	if( c == EOF )
    {
		stream->eof = 1;
		if( out == s )
        {
			return NULL;
		}else
        {
			return out;
		}
	}

	return NULL;
}
