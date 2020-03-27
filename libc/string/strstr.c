#include <string.h>
#include <sys/types.h>

static int compare( const char* haystack, const char* needle )
{
    while( *haystack && *needle )
    {
        if( *haystack != *needle )
        {
            return 0;
        }
        haystack++;
        needle++;
    }

    return *needle == '\0';
}

char* strstr( const char* haystack, const char* needle )
{
    while( *haystack != '\0' )
    {
        if( (*haystack && *needle) && compare(haystack, needle) )
        {
            return (char*)haystack;
        }
        haystack++;
    }

    return NULL;
}

char* strnstr( const char* haystack, const char* needle, size_t len )
{
    char c, sc;
    size_t clen;

    if( (c = *needle++) != '\0' )
    {
        clen = strlen(needle);
        do{
            do{
                if( len-- < 1 || (sc = *haystack++) == '\0' )
                {
                    return NULL;
                }
            }while( sc != c );
            if( clen > len )
            {
                return NULL;
            }
        }while( strncmp(haystack, needle, len ) != 0 );
    }

    return (char*)haystack;
}
