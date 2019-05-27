#ifndef _STRING_H
#define _STRING_H 1

#include <sys/cdefs.h>
#include <sys/types.h>
#include <stddef.h>

int memcmp( const void*, const void*, size_t );
void* memcpy( void* __restrict, const void* __restrict, size_t );
void* memmove( void*, const void*, size_t );
void* memset( void*, int, size_t );
size_t strlen( const char* );
int strcmp( const char*, const char* );
char* strcpy( char* restrict, const char* restrict );
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

char* strcat( char*, const char* );
char* strncat( char*, const char*, size_t );

void* memchr( const void*, int, size_t );

#endif
