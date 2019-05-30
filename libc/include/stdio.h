#ifndef _STDIO_H
#define _STDIO_H 1

#include <sys/cdefs.h>
#include <stddef.h>
#include <stdarg.h>

#define EOF (-1)

#define SEEK_SET 0
#define SEEK_CUR 1
#define SEEK_END 2

typedef struct _FILE FILE;

extern FILE* stdin;
extern FILE* stdout;
extern FILE* stderr;

int fopen( const char*, const char* );
int fclose( FILE* );
size_t fread( void*, size_t, size_t, FILE* );
size_t fwrite( const void*, size_t, size_t, FILE* );
int fseek( FILE*, long, int );
long ftell( FILE* );

void setbuf( FILE*, char* );
int fflush( FILE* );

int printf( const char* __restrict, ... );
int putchar( int );
int puts( const char* );
int sprintf( char* buf, const char* __restrict, ... );
int vsprintf( char* buf, const char* __restrict, va_list args );

int fprintf( FILE*, const char*, ... );
int vfprintf( FILE*, const char*, va_list );

#endif
