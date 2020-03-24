#include <stdio.h>
#include <unistd.h>
#include <debug.h>
#include <string.h>

int main( void )
{
    char buf[1024];
    
    while(1)
    {
        printf(">");
        int r = read(0, buf, 1024);

        if( r > 0 )
        {
            buf[r] = '\0';

            if( !strcmp(buf, "ls\n") )
            {
                debugvfstree();
            }else if( !strcmp(buf, "ps\n") )
            {
                debugproctree();
                
            }else if( !strcmp(buf, "exit\n") )
            {
                return 0;
            }else
            {
                printf("sh: Command not found: %s", buf);
            }
        }
    }
}
