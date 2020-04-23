#include <stdio.h>
#include "file.h"

void clearerr( FILE* stream )
{
    stream->eof = 0;
}
