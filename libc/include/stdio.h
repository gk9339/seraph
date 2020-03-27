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

//Output buffering
#define _IOFBF 0 //Full buffering
#define _IOLBF 1 //Line buffering
#define _IONBF 2 //No buffering

typedef struct _FILE FILE;

extern FILE* stdin;
extern FILE* stdout;
extern FILE* stderr;

FILE* fopen( const char*, const char* );
int fclose( FILE* );
size_t fread( void*, size_t, size_t, FILE* ); 
size_t fwrite( const void*, size_t, size_t, FILE* ); 
int fseek( FILE*, long, int ); 
long ftell( FILE* ); //stub

FILE* fdopen( int, const char* );
FILE* fmemopen( void*, size_t, const char* );

void setbuf( FILE*, char* ); 
void setbuffer( FILE*, char*, size_t );
void setlinebuf( FILE* );
int setvbuf( FILE*, char*, int, size_t );
int fflush( FILE* ); 
int fileno( FILE* );

int putc( int, FILE* );
int putchar( int );
int puts( const char* );

int printf( const char*, ... );
int fprintf( FILE*, const char*, ... );
int dprintf( int, const char*, ... );
int sprintf( char*, const char*, ... );
int snprintf( char*, size_t, const char*, ... );

int vprintf( const char*, va_list );
int vfprintf( FILE*, const char*, va_list );
int vdprintf( int, const char*, va_list );
int vsprintf( char*, const char*, va_list );
int vsnprintf( char*, size_t, const char*, va_list );

void perror( const char* );

#endif
