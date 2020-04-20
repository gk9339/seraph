#ifndef _STDLIB_H
#define _STDLIB_H

#ifdef __cplusplus
extern "C" {
#endif

#include <sys/cdefs.h>
#include <stdint.h>
#include <stddef.h>

#define EXIT_FAILURE 1
#define EXIT_SUCCESS 0

#define ATEXIT_MAX 128

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

int abs( int j );
// STUB
typedef struct
{
    int quotient;
    int remainder;
}div_t;

typedef struct
{
    long int quotient;
    long int remainder;
}ldiv_t;

div_t div( int numerator, int denominator );
ldiv_t ldiv( int numerator, int denominator );

double atof( const char* nptr );
long atol( const char* nptr );

void* bsearch( const void* key, const void* base, size_t nmemb, size_t size, int (*compar)(const void*, const void*));

long int labs( long int j );

int rand( void );
void srand( unsigned int );

double strtod( const char* nptr, char** endptr );
float strtof( const char* nptr, char** endptr );
long int strtol( const char* s, char** endptr, int base );
unsigned long int strtoul( const char* nptr, char** endptr, int base );

int system( const char* command );


#ifdef __cplusplus
}
#endif

#endif
