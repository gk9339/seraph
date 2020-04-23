#ifndef _STRINGS_H
#define _STRINGS_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stddef.h>

int strcasecmp( const char* s1, const char* s2 );
int strncasecmp( const char* s1, const char* s2, size_t n );

#ifdef __cplusplus
}
#endif

#endif
