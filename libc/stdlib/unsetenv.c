#include <string.h>
#include <stdlib.h>
#include <stdio.h>
#include <unistd.h>

int unsetenv( const char* string )
{
    int last_index = -1;
    int found_index = -1;
    int len = strlen(string);

    for( int i = 0; environ[i]; i++ )
    {
        if( found_index == -1 && (strstr(environ[i], string) == environ[i] && environ[i][len] == '=') )
        {
            found_index = i;
        }
        last_index = i;
    }

    if( found_index == -1 )
    {
        return 0;
    }

    if( last_index == found_index )
    {
        environ[last_index] = NULL;
        return 0;
    }

    environ[found_index] = environ[last_index];
    environ[last_index] = NULL;

    return 0;
}
