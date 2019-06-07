#include <unistd.h>
#include <sys/syscall.h>
#include <string.h>

DEFN_SYSCALL3(execve, SYS_EXECVE, char*, char**, char**)

extern char** environ;

int execve( const char* name, char* const argv[], char* const envp[] )
{
    return syscall_execve((char*)name, (char**)argv, (char**)envp);
}