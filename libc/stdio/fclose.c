#include <stdio.h>
#include <stdlib.h>
#include <sys/syscall.h>
#include "file.h"

int fclose( FILE* stream )
{
    int out = syscall_close(stream->fd);
    free(stream->_name);
    free(stream->read_base);

    if( stream == stdin || stream == stdout || stream == stderr )
    {
        return out;
    }else
    {
        free(stream);
        return out;
    }
}
