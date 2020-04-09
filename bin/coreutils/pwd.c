#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>

int main( void )
{
    char tmp[1024];
    if( getcwd(tmp, 1023) )
    {
        puts(tmp);
        return EXIT_SUCCESS;
    }else
    {
        return EXIT_FAILURE;
    }
}
