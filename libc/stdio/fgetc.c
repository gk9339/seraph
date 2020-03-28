#include <stdio.h>
#include "file.h"

int fgetc( FILE* stream )
{
    char buf[1];
    int r;

    r = fread(buf, 1, 1, stream);
    
    if( r < 0 )
    {
        stream->eof = 1;
        return EOF;
    }else if( r == 0 )
    {
        stream->eof = 1;
        return EOF;
    }

    return (unsigned char)buf[0];
}
