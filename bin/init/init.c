#include <unistd.h>
#include <sys/syscall.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <dirent.h>
#include <fcntl.h>
#include <sys/stat.h>
#include <sys/ioctl.h>
#include <sys/mount.h>

void copy_directory( char* source, char* dest, int mode, int uid, int gid );
void copy_link( char* source, char* dest, int mode, int uid, int gid );
void copy_file( char* source, char* dest, int mode, int uid, int gid );
void free_ramdisk( char * path );

int main( void )
{
    if( getpid() != 1 )
    {
        printf("Init already started.\nExiting\n");
        exit(0);
    }     

    // Remount ramdisk fs as tmpfs
    mount("/dev/ram0", "/dev/base", "ustar");
    mount("root,755", "/", "tmpfs");

    copy_directory("/dev/base","/",0660,0,0);
    mount("root,755", "/dev/base", "tmpfs");

    free_ramdisk("/dev/ram0");

    // Setup standard streams to point to /dev/null
    syscall_open("/dev/null", 0, 0);
    syscall_open("/dev/null", 1, 0);
    syscall_open("/dev/null", 1, 0);

    // Set initial hostname to /conf/hostname
    char hostname[256] = { 0 };
    int hostname_fd = open("/conf/hostname", O_RDONLY);
    if( hostname_fd > 0 )
    {
        read(hostname_fd, &hostname, 256);
        close(hostname_fd);
        strtok(hostname, "\n"); // Remove \n from the hostname
        sethostname(hostname, strlen(hostname));
    }

    // TODO: use /etc/init.d or similar for this
    pid_t pid = fork();

    if(!pid)
    {
        char* arg[] = { "/bin/terminal", NULL };
        char* env[] = { "PATH=/bin", "LD_LIBRARY_PATH=/lib", NULL};
        execve("/bin/terminal", arg, env);
    }

    while ((pid=waitpid(-1,NULL,WNOKERN))!=-1);

    sleep(1);
    syscall_reboot(1);
    __builtin_unreachable();
}

void copy_directory( char* source, char* dest, int mode, int uid, int gid )
{
    DIR* dirp = opendir(source);
    if( dirp == NULL )
    {
        fprintf(stderr, "Failed to copy directory %s\n", source);
        return;
    }

    if( !strcmp(dest, "/") )
    {
        dest = "";
    }else
    {
        mkdir(dest, mode);
    }

    struct dirent* ent = readdir(dirp);
    while( ent != NULL )
    {
        if( !strcmp(ent->d_name,".") || !strcmp(ent->d_name,"..") )
        {
            ent = readdir(dirp);
            continue;
        }

        struct stat statbuf;
        char tmp[strlen(source)+strlen(ent->d_name)+2];
        sprintf(tmp, "%s/%s", source, ent->d_name);
        char tmp2[strlen(dest)+strlen(ent->d_name)+2];
        sprintf(tmp2, "%s/%s", dest, ent->d_name);

        lstat(tmp,&statbuf);

        if( S_ISLNK(statbuf.st_mode) )
        {
            copy_link(tmp, tmp2, statbuf.st_mode & 07777, statbuf.st_uid, statbuf.st_gid);
        }else if( S_ISDIR(statbuf.st_mode) )
        {
            copy_directory(tmp, tmp2, statbuf.st_mode & 07777, statbuf.st_uid, statbuf.st_gid);
        }else if( S_ISREG(statbuf.st_mode) )
        {
            copy_file(tmp, tmp2, statbuf.st_mode & 07777, statbuf.st_uid, statbuf.st_gid);
        }else
        {
            fprintf(stderr, " %s is not any of the required file types\n", tmp);
        }
        ent = readdir(dirp);
    }
    closedir(dirp);

    chown(dest, uid, gid);
}

void copy_link( char* source, char* dest, int mode __attribute__((unused)), int uid, int gid )
{
    char tmp[1024];

    readlink(source, tmp, 1024);
    symlink(tmp, dest);
    chown(dest, uid, gid);
}

void copy_file( char* source, char* dest, int mode, int uid, int gid )
{
    int d_fd = open(dest, O_WRONLY | O_CREAT, mode);
    int s_fd = open(source, O_RDONLY);

    ssize_t length;

    length = lseek(s_fd, 0, SEEK_END);
    lseek(s_fd, 0, SEEK_SET);

    char buf[BUFSIZ];

    while( length > 0 )
    {
        size_t r = read(s_fd, buf, length < BUFSIZ ? length : BUFSIZ);
        write(d_fd, buf, r);
        length -= r;
    }

    close(s_fd);
    close(d_fd);

    chown(dest, uid, gid);
}

void free_ramdisk( char * path )
{
    int fd = open(path, O_RDONLY);
    ioctl(fd, 0x4001, NULL);
    close(fd);
}
