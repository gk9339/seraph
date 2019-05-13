#ifndef _TIME_H
#define _TIME_H

#include <sys/types.h>

struct timeval
{
    time_t tv_sec;
    suseconds_t tv_usec;
};

struct timezone
{
    int tz_minuteswest;
    int tz_dsttime;
};

int gettimeofday( struct timeval* p, void* z );

#endif
