#ifndef _UNISTD_H
#define _UNISTD_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stddef.h>
#include <stdint.h>
#include <sys/types.h>

// Standard file descriptors
#define STDIN_FILENO  0 // Standard input
#define STDOUT_FILENO 1 // Standard output
#define STDERR_FILENO 2 // Standard error output

extern char** environ;
extern int _environ_size;

extern void _exit( int status );

pid_t fork( void );

void* sbrk( intptr_t );

pid_t getpid( void );
pid_t getppid( void );
uid_t getuid( void );
uid_t geteuid( void );
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
ssize_t readlink( const char* path, char* buf, size_t bufsize );
int symlink( const char* target, const char* linkpath );
int link( const char* path1, const char* path2 );
int unlink( const char* path );

off_t lseek( int fd, off_t offset, int whence );

int close( int fd );

int usleep( useconds_t usec );
int sleep( unsigned int sec );

int chdir( const char* path );
char* getwd( char* buf );
char* getcwd( char* buf, size_t size );

int dup( int oldfd );
int dup2( int oldfd, int newfd );
int pipe( int fd[2] );

pid_t tcgetpgrp( int fd );
int tcsetpgrp( int fd, pid_t pgrp );

int sethostname( const char* name, size_t len );
int gethostname( char* name, size_t len );

int isatty( int fd );

struct utimbuf
{
    time_t actime;
    time_t modtime;
};
int utime( const char* filename, const struct utimbuf* times );
int chown( const char* pathname, uid_t owner, gid_t group );
int rmdir( const char* path );

extern char* optarg;
extern int optind, opterr, optopt;
int getopt( int argc, char* const argv[], const char* optstring );

int getpagesize( void );

int access( const char* pathname, int mode );

unsigned int alarm( unsigned int sec );

#ifdef __cplusplus
}
#endif

#endif
