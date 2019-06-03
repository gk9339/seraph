#ifndef _UNISTD_H
#define _UNISTD_H

#include <stddef.h>
#include <sys/types.h>

pid_t fork( void );

void* sbrk( intptr_t );

int execv( const char*, char* const argv[] );
int execve( const char*, char* const argv[], char* const envp[] );
int execvp( const char*, char* const argv[] );

ssize_t write( int fd, const void* buf, size_t count );
ssize_t read( int fd, void* buf, size_t count );

int close( int fd );

int usleep( useconds_t usec );
int sleep( unsigned int sec );

#endif
