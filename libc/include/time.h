#ifndef _TIME_H
#define _TIME_H

#ifdef __cplusplus
extern "C" {
#endif

#include <sys/time.h>
#include <stddef.h>

typedef int clock_t;

struct tm
{
    int tm_sec;    // Seconds (0-60)
    int tm_min;    // Minutes (0-59)
    int tm_hour;   // Hours (0-23)
    int tm_mday;   // Day of the month (1-31)
    int tm_mon;    // Month (0-11)
    int tm_year;   // Year - 1900
    int tm_wday;   // Day of the week (0-6, Sunday = 0)
    int tm_yday;   // Day in the year (0-365, 1 Jan = 0)
    int tm_isdst;  // Daylight saving time
};

clock_t clock( void );

double difftime( time_t a, time_t b );
time_t mktime( struct tm* tm );
time_t time( time_t* out );
char* asctime( const struct tm* tm );
char* ctime( const time_t* timep );
struct tm* gmtime( const time_t* timep );
struct tm* gmtime_r( const time_t* timep, struct tm* result );
struct tm* localtime( const time_t* timep );
struct tm* localtime_r( const time_t* timep, struct tm* result );
size_t strftime( char* s, size_t max, const char* fmt, const struct tm* tm );

#ifdef __cplusplus
}
#endif

#endif
