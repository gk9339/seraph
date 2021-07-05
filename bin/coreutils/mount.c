#include <stdio.h>
#include <errno.h>
#include <sys/mount.h>
#include <stdlib.h>
#include <string.h>

int main( int argc, char** argv )
{
    if( argc < 4 )
    {
        fprintf(stderr, "Usage: %s type device mountpoint\n", argv[0]);
        return EXIT_FAILURE;
    }

    int ret = mount(argv[2], argv[3], argv[1]);

    if( ret < 0 )
    {
        fprintf(stderr, "%s: %s\n", argv[0], strerror(errno));
    }

    return EXIT_SUCCESS;
}
