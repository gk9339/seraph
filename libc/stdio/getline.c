#include <stdio.h>
#include <errno.h>
#include <string.h>
#include <stdlib.h>

ssize_t getline( char** lineptr, size_t* n, FILE* stream )
{
    static char line[BUFSIZ];
    char* ptr;
    unsigned int len;

    if( lineptr == NULL || n == NULL )
    {
       errno = EINVAL;
       return -1;
    }

    if( ferror (stream) )
    {
       return -1;
    }

    if( feof(stream) )
    {
       return -1;
    }

    if( !fgets(line,BUFSIZ,stream) )
    {
        return -1;
    }

    ptr = strchr(line,'\n');
    if( ptr )
    {
       *ptr = '\0';
    }

    len = strlen(line);

    if( (len+1) < BUFSIZ )
    {
       ptr = realloc(*lineptr, BUFSIZ);
       if( ptr == NULL )
       {
          return(-1);
       }
       *lineptr = ptr;
       *n = BUFSIZ;
    }

    strcpy(*lineptr,line);
    return(len);
}
