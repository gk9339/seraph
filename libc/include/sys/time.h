#ifndef _SYS_TIME_H
#define _SYS_TIME_H

#ifdef __cplusplus
extern "C" {
#endif

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

int gettimeofday( struct timeval* tv, void* tzp );

#ifdef __cplusplus
}
#endif

#endif
