#ifndef _STDLIB_H
#define _STDLIB_H 1

#include <sys/cdefs.h>
#include <sys/types.h>

__attribute__((__noreturn__))
void abort( void );

void* __attribute__((malloc)) malloc( uintptr_t size );
void* __attribute__((malloc)) realloc( void* ptr, uintptr_t size );
void* __attribute__((malloc)) calloc( uintptr_t nmemb, uintptr_t size );
void* __attribute__((malloc)) valloc( uintptr_t size );
void free( void* ptr );

long atoi( const char* );

int atexit(void (*function)(void));

#endif
