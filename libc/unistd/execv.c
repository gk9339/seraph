#include <unistd.h>
#include <sys/syscall.h>
#include <sys/stat.h>
#include <string.h>
#include <stdlib.h>
#include <errno.h>

DEFN_SYSCALL3(execve, SYS_EXECVE, const char*, char* const*, char* const*)

int execv( const char* path, char* const argv[] )
{
    return execve(path, argv, environ);
}

int execvp( const char* file, char* const argv[] )
{
    if( file && (!strstr(file, "/")) )
    {
        char* path = getenv("PATH");
        if( path == NULL )
        {
            path = "/bin";
        }
        char* xpath = strdup(path);
        char* p, *last;
        for( p = strtok_r(xpath, ":", &last); p; p = strtok_r(NULL, ":", &last) )
        {
            int r;
            struct stat stat_buf;
            char* exe = malloc(strlen(p) + strlen(file) + 2);
            strcpy(exe, p);
            strcat(exe, "/");
            strcat(exe, file);

            r = stat(exe, &stat_buf);
            if( r != 0 )
            {
                continue;
            }
            if( !(stat_buf.st_mode & S_IXUSR) )
            {
                continue;
            }

            return execve(exe, argv, environ);
        }
        free(xpath);
        errno = ENOENT;
        return -1;
    }else if( file )
    {
        return execve(file, argv, environ);
    }

    errno = ENOENT;
    return -1;
}

int execve( const char* path, char* const argv[], char* const envp[] )
{
    __sets_errno(syscall_execve(path, argv, envp));
}
