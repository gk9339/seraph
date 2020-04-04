#ifndef _CDEF_H
#define _CDEF_H

static inline int isdigit( int c )
{
    return (unsigned int)c-'0' < 10;
}

static inline int isspace( int c )
{
    return c == ' ' || (unsigned int)c-'\t' < 5;
}

#endif
