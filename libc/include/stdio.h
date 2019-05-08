#ifndef _STDIO_H
#define _STDIO_H 1

#include <sys/cdefs.h>

#define EOF (-1)

int printf( const char* __restrict, ... );
int putchar( int );
int puts( const char* );
int sprintf( char* buf, const char* __restrict, ... );

#endif
