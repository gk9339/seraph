#ifndef _CDEF_H
#define _CDEF_H

static inline int isdigit( int ch )
{
    return (unsigned int)ch-'0' < 10;
}

#endif
