#include <stdio.h>
#include "file.h"

int feof( FILE* stream )
{
    return stream->eof;
}
