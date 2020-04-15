#ifndef _STDLIB_H
#define _STDLIB_H 1

#include <sys/cdefs.h>
#include <stdint.h>
#include <stddef.h>

#define EXIT_FAILURE 1
#define EXIT_SUCCESS 0

#define ATEXIT_MAX 32

void __attribute__((__noreturn__)) abort( void );

char* getenv( const char* name );
int putenv( char* name );
int setenv( const char* name, const char* value, int overwrite );
int unsetenv( const char* str );

void* __attribute__((malloc)) malloc( uintptr_t size );
void* __attribute__((malloc)) realloc( void* ptr, uintptr_t size );
void* __attribute__((malloc)) calloc( uintptr_t nmemb, uintptr_t size );
void* __attribute__((malloc)) valloc( uintptr_t size );
void free( void* ptr );

int atoi( const char* );

int atexit(void (*function)(void));
void exit( int );

void qsort( void* base, size_t nmemb, size_t size, int(*compar)(const void*, const void*));

#endif
