#include "sh.h"

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

void ls( char* argv )
{
    struct dirent* de;
    DIR* dr = NULL;

    char* token = strtok(argv, " ");
    token = strtok(NULL, " ");

    if( token == NULL )
    {
        dr = opendir(".");
    }else
    {
        token[strlen(token)-1] = '\0';
        dr = opendir(token);
    }

    if( dr == NULL )
    {
        printf("Couldn't open root.\n");
    }else
    {
        while( (de = readdir(dr)) != NULL )
        {
            printf("%s\n", de->d_name);
        }
        closedir(dr);
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
