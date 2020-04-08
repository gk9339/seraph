#ifndef _UNISTD_H
#define _UNISTD_H

#include <stddef.h>
#include <stdint.h>
#include <sys/types.h>

// Standard file descriptors
#define STDIN_FILENO  0 // Standard input
#define STDOUT_FILENO 1 // Standard output
#define STDERR_FILENO 2 // Standard error output

typedef long ssize_t;

extern char** environ;
extern int _environ_size;

pid_t fork( void );

void* sbrk( intptr_t );

pid_t getpid( void );
pid_t getppid( void );
uid_t getuid( void );
gid_t getgid( void );

pid_t setsid( void );

int setuid( uid_t );
int setpgid( pid_t, pid_t );
int getpgid( pid_t );

int execv( const char*, char* const argv[] );
int execve( const char*, char* const argv[], char* const envp[] );
int execvp( const char*, char* const argv[] );

ssize_t write( int fd, const void* buf, size_t count );
ssize_t read( int fd, void* buf, size_t count );

off_t lseek( int fd, off_t offset, int whence );

int close( int fd );

int usleep( useconds_t usec );
int sleep( unsigned int sec );

int chdir( const char* path );

int dup( int oldfd );
int dup2( int oldfd, int newfd );

int pipe( int fd[2] );

int sethostname( const char* name, size_t len );
int gethostname( char* name, size_t len );

extern char* optarg;
extern int optind, opterr, optopt;
int getopt( int argc, char* const argv[], const char* optstring );

#endif
