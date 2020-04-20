#include <stdio.h>

int fputc( int c, FILE* stream )
{
    char str[] = {(char)c};
    if( fwrite(str, 1, 1, stream) == 1 )
    {
        return c;
    }else
    {
        return EOF;
    }
}
