#include "sh.h"

int main( void )
{
    char buf[1024];
    setlinebuf(stdout);

    while(1)
    {
        printf(">");
        fflush(stdout);
        int r = read(0, buf, 1024);

        if( r > 0 )
        {
            buf[r] = '\0';

            if( !strcmp(buf, "lvfs\n") )
            {
                lvfs();
            }else if( !strncmp(buf, "ls", 2) )
            {
                ls(buf);
            }else if( !strcmp(buf, "ps\n") )
            {
                ps();
            }else if( !strcmp(buf, "exit\n") )
            {
                return 0;
            }else if( !strcmp(buf, "clear\n") )
            {
                kill(getppid(), SIGUSR1);
            }else if( strcmp(buf, "\n") )
            {
                printf("sh: Command not found: %s", buf);
            }
        }
    }
}

void ps( void )
{
    char* str = calloc(4096, sizeof(char));
       
    debugproctree(&str);

    printf("%s", str);
}

void lvfs( void )
{
    char* str = calloc(4096, sizeof(char));
       
    debugvfstree(&str);

    printf("%s", str);
}
