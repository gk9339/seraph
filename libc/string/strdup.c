#include <string.h>
#include <stdlib.h>

/* duplicate a string */
char* strdup( const char* s )
{
    size_t len = strlen(s);
 
    return memcpy(malloc(len+1), s, len+1);
}
