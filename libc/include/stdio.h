#ifndef _STDIO_H
#define _STDIO_H

#ifdef __cplusplus
extern "C" {
#endif

#include <sys/cdefs.h>
#include <sys/types.h>
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

int getc( FILE* );
int getchar( void );
int fgetc( FILE* );

ssize_t getline( char**, size_t*, FILE* );

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

// STUB
typedef long fpos_t;

void clearerr( FILE* stream );
int feof( FILE* stream );
int ferror( FILE* stream );

int fgetpos( FILE* stream, fpos_t* pos );
int fsetpos( FILE* stream, const fpos_t* pos );

char* fgets( char* s, int size, FILE* stream );
int fputc( int c, FILE* stream );
int fputs( const char* s, FILE* stream );
FILE* freopen( const char* path, const char* mode, FILE* stream );

int remove( const char* pathname );
int rename( const char* oldpath, const char* newpath );

void rewind( FILE* stream );

int scanf( const char* fmt, ... );
int sscanf( const char* str, const char* fmt, ... );
int fscanf( FILE* stream, const char* fmt, ... );

int vscanf( const char* fmt, va_list );
int vsscanf( const char* str, const char* fmt, va_list );
int vfscanf( FILE* stream, const char* fmt, va_list );

FILE* tmpfile( void );
int ungetc( int c, FILE* stream );

#ifdef __cplusplus
}
#endif

#endif
