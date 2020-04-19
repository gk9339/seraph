#include <unistd.h>
#include <stdio.h>
#include <sys/wait.h>

int main( void )
{
    while( !fork() );

    return 0;
}
