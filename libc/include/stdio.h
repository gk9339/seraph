#ifndef _STDIO_H
#define _STDIO_H 1

#include <sys/cdefs.h>
#include <stddef.h>
#include <stdarg.h>

#define EOF (-1)

#define BUFSIZ 8192

#define SEEK_SET 0
#define SEEK_CUR 1
#define SEEK_END 2

#define _IO_NO_READS 0x0001
#define _IO_NO_WRITES 0x0002
#define _IO_APPEND 0x0004

typedef struct _FILE FILE;

extern FILE* stdin;
extern FILE* stdout;
extern FILE* stderr;

FILE* fopen( const char*, const char* );
int fclose( FILE* ); //stub
size_t fread( void*, size_t, size_t, FILE* ); 
size_t fwrite( const void*, size_t, size_t, FILE* ); 
int fseek( FILE*, long, int ); 
long ftell( FILE* ); //stub

FILE* fdopen( int, const char* );

void setbuf( FILE*, char* ); //stub
int fflush( FILE* ); //stub
int fileno( FILE* );

int printf( const char* __restrict, ... );
int putchar( int );
int puts( const char* );
int sprintf( char* buf, const char* __restrict, ... );
int vsprintf( char* buf, const char* __restrict, va_list args );

int fprintf( FILE*, const char*, ... ); //stub
int vfprintf( FILE*, const char*, va_list ); //stub

#endif
