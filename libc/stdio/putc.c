#include <stdio.h>

int putc( int c, FILE* stream )
{
    return fwrite(&c, sizeof(char), 1, stream);
}
