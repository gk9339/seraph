#include <stdlib.h>
#include <ctype.h>

int atoi( const char* nptr )
{
    int n = 0;
    int neg = 0;

    while( isspace(*nptr) )
    {
        nptr++;
    }

    switch(*nptr)
    {
        case '-':
            neg = 1;
            __attribute__((fallthrough));
        case '+':
            nptr++;
    }

    while( isdigit(*nptr) )
    {
        n = 10 * n - (*nptr++ - '0');
    }

    return neg ? n : -n;
}
