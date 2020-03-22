#include <string.h>
#include <stdlib.h>
#include <stdio.h>
#include <unistd.h>

int putenv( char* string )
{
    char name[strlen(string)];
    strcpy(name, string);
    char* c = strchr(name, '=');
    if( !c )
    {
        return 1;
    }
    *c = '\0';

    int s = strlen(name);

    int i;
    for( i = 0; i < (_environ_size - 1) && environ[i]; i++ )
    {
        if( !strnstr(name, environ[i], s) && environ[i][s] == '=' )
        {
            environ[i] = string;
            return 0;
        }
    }

    if( i == _environ_size - 1 )
    {
        int j = 0;
        int _new_environ_size = _environ_size + 2;
        char** new_environ = malloc(sizeof(char*) * _new_environ_size);

        while( j < _new_environ_size && environ[j] )
        {
            new_environ[j] = environ[j];
            j++;
        }

        while( j < _new_environ_size )
        {
            new_environ[j] = NULL;
            j++;
        }

        _environ_size = _new_environ_size;
        environ = new_environ;
    }

    environ[i] = string;

    return 0;
}
