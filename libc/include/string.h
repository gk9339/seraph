#ifndef _STRING_H
#define _STRING_H

#ifdef __cplusplus
extern "C" {
#endif

#include <sys/cdefs.h>
#include <stddef.h>

int memcmp( const void*, const void*, size_t );
void* memcpy( void* __restrict, const void* __restrict, size_t );
void* memmove( void*, const void*, size_t );
void* memset( void*, int, size_t );
size_t strlen( const char* );
size_t strnlen( const char*, size_t );
int strcmp( const char*, const char* );
int strncmp( const char*, const char*, size_t );
char* strcpy( char* dest, const char* src );
char* strdup( const char* );

char* strtok( char*, const char* );
char* strtok_r( char*, const char*, char** );

size_t strspn( const char*, const char* );
size_t strcspn( const char*, const char* );
size_t lfind( const char*, const char );

char* strpbrk( const char*, const char* );

char* strchr( const char*, int );
char* strchrnul( const char*, int );

char* strstr( const char*, const char* );
char* strnstr( const char*, const char*, size_t );

char* strcat( char*, const char* );
char* strncat( char*, const char*, size_t );

void* memchr( const void*, int, size_t );

char* strerror( int );
char* strsignal( int );

// STUB
int strcoll( const char* s1, const char* s2 );
char* strncpy( char* dest, const char* src, size_t n );
size_t strxfrm( char* dest, const char* src, size_t n );
char* strrchr( const char* s, int c );

#ifdef __cplusplus
}
#endif

#endif
