#include <stdio.h>
#include <unistd.h>

int main( void )
{
    unsigned char buf[1024];

    while(1)
    {
        int r = read(0, buf, 1024);

        if( r > 0 )
        {
            buf[r] = '\0';
            printf("Input: %s", buf);
        }
    }
}
