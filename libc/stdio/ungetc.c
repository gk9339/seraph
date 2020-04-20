#include <stdio.h>
#include "file.h"

int ungetc( int c, FILE* stream )
{
    if( stream->ungetc > 0 )
    {
        return EOF;
    }

    stream->ungetc = (char)c;

    return c;
}
