#ifndef _UNISTD_H
#define _UNISTD_H

#include <stddef.h>
#include <sys/types.h>

pid_t fork( void );

int execv( const char*, char* const argv[] );
int execve( const char*, char* const argv[], char* const envp[] );
int execvp( const char*, char* const argv[] );

#endif
