#include <kernel/irq.h>
#include <kernel/timer.h>
#include <kernel/isr.h>
#include <sys/types.h>
#include <kernel/types.h>
#include <kernel/process.h>
#include <kernel/task.h>
#include <kernel/serial.h>
#include <kernel/syscall.h>
#include <kernel/spinlock.h>
#include <sys/syscall.h>
#include <errno.h>
#include <stdlib.h>
#include <fcntl.h>
#include <kernel/fs.h>
#include <stdio.h>
#include <sys/utsname.h>
#include <kernel/elf.h>
#include <kernel/idt.h>
#include <kernel/shm.h>
#include <kernel/pty.h>
#include <kernel/signal.h>
#include <kernel/unixpipe.h>
#include <kernel/version.h>
#include <kernel/acpi.h>

static char hostname[256] = { 0 };
static size_t hostname_len = 0;

#define FD_INRANGE(FD) ((FD) < (int)current_process->fds->length && (FD) >= 0)
#define FD_ENTRY(FD) (current_process->fds->entries[(FD)])
#define FD_CHECK(FD) (FD_INRANGE(FD) && FD_ENTRY(FD))
#define FD_OFFSET(FD) (current_process->fds->offsets[(FD)])
#define FD_FLAG(FD) (current_process->fds->flags[(FD)])

#define PTR_INRANGE(PTR) ((uintptr_t)(PTR) > current_process->image.entry)
#define PTR_VALIDATE(PTR) ptr_validate((void *)(PTR), __func__)

#define MIN(A, B) ((A) < (B)?(A):(B))

void ptr_validate( void* ptr, const char* syscall )
{
    if( ptr && !PTR_INRANGE(ptr) )
    {
        char debug_str[256];
        debug_logf(debug_str, "SEGFAULT: invalid pointer passsed to %s (0x%x < 0x%x)", syscall, (uintptr_t)ptr, current_process->image.entry);
        KPANIC("Segmentation fault", NULL);
    }
}

static int __attribute__((noreturn)) sys_exit( int retval )
{
    task_exit((retval & 0xFF) << 8);
    while(1);
}

static int sys_open( const char* file, int flags, uint16_t mode )
{
    PTR_VALIDATE(file);
    fs_node_t* node = kopen((char*)file, flags);

    if( node && (flags & FS_O_CREAT) && (flags & FS_O_EXCL) )
    {
        close_fs(node);
        return -EEXIST;
    }

    if( node && (!(flags & FS_O_WRONLY) || (flags & FS_O_RDWR)) )
    {
        if( node && !has_permission(node, S_IROTH) )
        {
            close_fs(node);
            return -EACCES;
        }
    }

    if( node && ((flags & FS_O_RDWR) || (flags & FS_O_WRONLY)) )
    {
        if( node && !has_permission(node, S_IWOTH) )
        {
            close_fs(node);
            return -EACCES;
        }
        
        if( node &&  (node->type & FS_DIRECTORY) )
        {
            return -EISDIR;
        }
    }

    if( !node && (flags & O_CREAT) )
    {
        int result = create_file_fs((char*)file, mode);
        if( !result )
        {
            node = kopen((char*)file, mode);
        }else
        {
            return result;
        }
    }

    if( node && (flags & O_DIRECTORY) )
    {
        if( !(node->type & FS_DIRECTORY) )
        {
            return -ENOTDIR;
        }
    }

    if( node && (flags & O_TRUNC) )
    {
        if( !(flags & O_WRONLY || flags & O_RDWR) ) 
        {
            close_fs(node);
            return -EINVAL;
        }
        truncate_fs(node);
    }

    if( !node )
    {
        return -ENOENT;
    }

    if( node && (flags & O_CREAT) && (node->type & FS_DIRECTORY) )
    {
        close_fs(node);
        return -EISDIR;
    }

    int fd = process_append_fd((process_t*)current_process, node);
    FD_FLAG(fd) = flags;
    if( flags & FS_O_APPEND )
    {
        FD_OFFSET(fd) = node->length;
    }else
    {
        FD_OFFSET(fd) = 0;
    }

    return fd;
}

static int sys_read( int fd, char* ptr, int len )
{
    if( FD_CHECK(fd) )
    {
        PTR_VALIDATE(ptr);

        fs_node_t* node = FD_ENTRY(fd);
        if( (FD_FLAG(fd) & O_ACCMODE) == O_RDONLY || (FD_FLAG(fd) & O_ACCMODE) == O_RDWR ) //fd read flag
        {
            uint32_t out = read_fs(node, (uint32_t)FD_OFFSET(fd), len, (uint8_t*)ptr);
            FD_OFFSET(fd) += out;

            return (int)out;
        }else
        {
            return -EACCES;
        }
    }

    return -EBADF;
}

static int sys_write( int fd, char* ptr, int len )
{
    if( FD_CHECK(fd) )
    {
        PTR_VALIDATE(ptr);
        fs_node_t* node = FD_ENTRY(fd);
        if( FD_FLAG(fd) &  O_WRONLY || FD_FLAG(fd) &  O_RDWR ) //fd write flag
        {
            uint32_t out = write_fs(node, (uint32_t)FD_OFFSET(fd), len, (uint8_t*)ptr);
            FD_OFFSET(fd) += out;

            return out;
        }
    }

    return -EBADF;
}

static int sys_close( int fd )
{
    if( FD_CHECK(fd) )
    {
        close_fs(FD_ENTRY(fd));
        FD_ENTRY(fd) = NULL;

        return 0;
    }
    
    return -EBADF;
}

static int sys_gettimeofday( struct timeval* tv, void* tzp )
{
    PTR_VALIDATE(tv);
    PTR_VALIDATE(tzp);

    return gettimeofday(tv, tzp);
}

static int sys_execve( const char* filename, char* const argv[], char* const envp[] )
{
    PTR_VALIDATE(argv);
    PTR_VALIDATE(filename);
    PTR_VALIDATE(envp);

    int argc = 0;
    int envc = 0;
    while( argv[argc] )
    {
        PTR_VALIDATE(argv[argc]);
        argc++;
    }
    if( envp )
    {
        while( envp[envc] )
        {
            PTR_VALIDATE(envp[envc]);
            envc++;
        }
    }

    char ** argv_ = malloc(sizeof(char *) * (argc + 1));

    for( int j = 0; j < argc; j++ )
    {
        argv_[j] = malloc((strlen(argv[j]) + 1) * sizeof(char));
        memcpy(argv_[j], argv[j], strlen(argv[j]) + 1);
    }
    
    argv_[argc] = 0;
    char ** envp_;
    if( envp && envc )
    {
        envp_ = malloc(sizeof(char *) * (envc + 1));
        for( int j = 0; j < envc; j++ )
        {
            envp_[j] = malloc((strlen(envp[j]) + 1) * sizeof(char));
            memcpy(envp_[j], envp[j], strlen(envp[j]) + 1);
        }
        envp_[envc] = 0;
    }else
    {
        envp_ = malloc(sizeof(char *));
        envp_[0] = NULL;
    }

    shm_release_all((process_t*)current_process);

    current_process->cmdline = argv_;

    return exec((char*)filename, argc, (char**)argv_, (char**)envp_);
}

static int sys_fork( void )
{
    return (int)fork();
}

static int sys_clone( uintptr_t new_stack, uintptr_t thread_func, uintptr_t arg )
{
    if( !new_stack || !PTR_INRANGE(new_stack) )
    {
        return -EINVAL;
    }
    if( !thread_func || !PTR_INRANGE(thread_func) )
    {
        return -EINVAL;
    }

    return (int)clone(new_stack, thread_func, arg);
}

static int sys_getpid( void )
{
    if( current_process->group )
    {
        return current_process->group;
    }else
    {
        return current_process->id;
    }
}

static int sys_sbrk( int size )
{
    process_t* proc = (process_t*)current_process;
    if( proc->group != 0 )
    {
        proc = process_from_pid(proc->group);
    }

    spin_lock(proc->image.lock);
    uintptr_t ret = proc->image.heap;
    uintptr_t i_ret = ret;
    ret = (ret + 0xffff) & ~0xfff; /* rounds to 0x1000 */
    proc->image.heap += (ret - i_ret) + size;
    while( proc->image.heap > proc->image.heap_actual )
    {
        proc->image.heap_actual += 0x1000;
        alloc_frame(get_page(proc->image.heap_actual, 1, current_directory), 0, 1);
        invalidate_tables_at(proc->image.heap_actual);
    }
    spin_unlock(proc->image.lock);

    return ret;
}

static int sys_uname( struct utsname* name )
{
    PTR_VALIDATE(name);

    char release[256];
    sprintf(release, "%d.%d.%d", _kernel_version_major, _kernel_version_minor, _kernel_version_lower);

    char version[256];
    sprintf(version, "%s %s %s", _kernel_build_date, _kernel_build_time, _kernel_build_timezone);
    
    strcpy(name->sysname, _kernel_name);
    strcpy(name->nodename, hostname);
    strcpy(name->release, release);
    strcpy(name->version, version);
    strcpy(name->machine, _kernel_arch);
    strcpy(name->domainname, "");

    return 0;
}

static int sys_sethostname( char* new_hostname, size_t len )
{
    if( current_process->user == USER_ROOT_UID )
    {
        PTR_VALIDATE(new_hostname);

        size_t slen = strlen(new_hostname) + 1;
        if( slen > 256 || len > 256 || len <= 0 )
        {
            return -EINVAL;
        }

        hostname_len = len;
        memcpy(hostname, new_hostname, len);
        
        if( len < slen )
        {
            new_hostname[len] = '\0';
        }

        return 0;
    }else
    {
        return -EPERM;
    }
}

static int sys_gethostname( char* buffer, size_t len )
{
    PTR_VALIDATE(buffer);
    if( len < hostname_len )
    {
         return -ENAMETOOLONG;
    }
    if( len > 256 || len <= 0 )
    {
        return -EINVAL;
    }

    memcpy(buffer, hostname, len);
    return hostname_len;
}

static int sys_mkdir( char* path, uint16_t mode )
{
    return mkdir_fs(path, mode);
}

static int sys_kill( pid_t pid, uint32_t sig )
{
    if( pid < -1 )
    {
        return group_send_signal(-pid, sig, 0);
    }else if( pid == 0 )
    {
        return group_send_signal(current_process->job, sig, 0);
    }else
    {
        return send_signal(pid, sig, 0);
    }
}

static int sys_signal( uint32_t signum, uintptr_t handler )
{
    if( signum > NUMSIGNALS )
    {
        return -EINVAL;
    }

    uintptr_t old = current_process->signals.functions[signum];
    current_process->signals.functions[signum] = handler;
    return (int)old;
}

static int sys_gettid( void )
{
    return getpid();
}

static int sys_openpty( int* master, int* slave, char* name __attribute__((unused)), void* termios, void* winsize )
{
    if( !master || !slave ) return -EINVAL;
    if( master && !PTR_INRANGE(master) ) return -EINVAL;
    if( slave && !PTR_INRANGE(slave) ) return -EINVAL;
    if( winsize && !PTR_INRANGE(winsize) ) return -EINVAL;

    fs_node_t* fs_master;
    fs_node_t* fs_slave;

    pty_create(termios, winsize, &fs_master, &fs_slave);

    *master = process_append_fd((process_t*)current_process, fs_master);
    *slave = process_append_fd((process_t*)current_process, fs_slave);

    FD_FLAG(*master) = FS_O_RDWR;
    FD_FLAG(*slave) = FS_O_RDWR;

    open_fs(fs_master, 0);
    open_fs(fs_slave, 0);

    return 0;
}

static int sys_seek( int fd, int offset, int whence )
{
    if( FD_CHECK(fd) )
    {
        if( (FD_ENTRY(fd)->type & FS_PIPE) || (FD_ENTRY(fd)->type & FS_CHARDEVICE) )
        {
            return -ESPIPE;
        }

        switch( whence )
        {
            case 0:
                FD_OFFSET(fd) = offset;
                break;
            case 1:
                FD_OFFSET(fd) += offset;
                break;
            case 2:
                FD_OFFSET(fd) = FD_ENTRY(fd)->length + offset;
                break;
            default:
                return -EINVAL;
        }

        return (int)FD_OFFSET(fd);
    }

    return -EBADF;
}

static int sys_readlink( const char* path, char* ptr, size_t len )
{
    PTR_VALIDATE(path);
    fs_node_t* node = kopen((char*)path, FS_O_PATH | FS_O_NOFOLLOW);
    if( !node )
    {
        return -ENOENT;
    }

    int retval = readlink_fs(node, ptr, len);
    close_fs(node);

    return retval;
}

static int stat_node( fs_node_t* fn, uintptr_t st )
{
    struct stat* f = (struct stat*)st;
    PTR_VALIDATE(f);

    if( !fn )
    {
        memset(f, 0, sizeof(struct stat));
        return -ENOENT;
    }

    f->st_dev = (uint16_t)(((uint32_t)fn->device * 0xFFFF0) >> 8);
    f->st_ino = fn->inode;

    uint32_t type = 0;
    if( fn->type & FS_FILE )
    {
        type |= _IFREG;
    }
    if( fn->type & FS_DIRECTORY )
    {
        type |= _IFDIR;
    }
    if( fn->type & FS_CHARDEVICE )
    {
        type |= _IFCHR;
    }
    if( fn->type & FS_BLOCKDEVICE )
    {
        type |= _IFBLK;
    }
    if( fn->type & FS_PIPE )
    {
        type |= _IFIFO;
    }
    if( fn->type & FS_SYMLINK )
    {
        type |= _IFLNK;
    }

    f->st_mode = fn->mask | type;
    f->st_nlink = fn->nlink;
    f->st_uid = fn->uid;
    f->st_gid = fn->gid;
    f->st_rdev = 0;
    f->st_size = fn->length;

    f->st_atime = fn->atime;
    f->st_mtime = fn->mtime;
    f->st_ctime = fn->ctime;
    f->st_blksize = 512;

    if (fn->get_size) {
        f->st_size = fn->get_size(fn);
    }

    return 0;
}

static int sys_stat( int fd, uintptr_t st )
{
    PTR_VALIDATE(st);
    
    if( FD_CHECK(fd) )
    {
        return stat_node(FD_ENTRY(fd), st);
    }

    return -EBADF;
}

static int sys_statf( char* file, uintptr_t st )
{
    int result;
    PTR_VALIDATE(file);
    PTR_VALIDATE(st);

    fs_node_t* fn = kopen(file, 0);
    result = stat_node(fn, st);

    if( fn )
    {
        close_fs(fn);
    }

    return result;
}

static int sys_lstat( char* file, uintptr_t st )
{
    int result;
    PTR_VALIDATE(file);
    PTR_VALIDATE(st);

    fs_node_t* fn = kopen(file, FS_O_PATH | FS_O_NOFOLLOW);
    result = stat_node(fn, st);

    if( fn )
    {
        close_fs(fn);
    }

    return result;
}

static int sys_dup2( int oldfd, int newfd )
{
    return process_move_fd((process_t*)current_process, oldfd, newfd);
}

static int sys_getuid( void )
{
    return current_process->real_user;
}

static int sys_reboot( int type )
{
    if( current_process->user != USER_ROOT_UID )
    {
        return -EPERM;
    }else
    {
        int_disable();
        
        if( type == 0 )
        {
            // 8042 reset
            uint8_t out = 0x02; // Clear all keyboard buffers
            while( (out & 0x02) != 0 )
            {
                out = inportb(0x64);
            }
            outportb(0x64, 0xFE); // Keyboard reset command
        }else if( type == 1 )
        {
            acpi_poweroff();
            debug_log("acpi poweroff failed - trying QEMU poweroff");
            outports(0x604, 0x0 | 0x2000);
            debug_log("QEMU poweroff failed - trying Bochs poweroff");
            outports(0xB004, 0x0 | 0x2000);
            debug_log("Bochs poweroff failed - trying Virtualbox poweroff");
            outports(0x4004, 0x0 | 0x3400);
            debug_log("Virtualbox poweroff failed - rebooting...");
            sys_reboot(0);
        }
        STOP;
    }

    return 0;
}

static int sys_readdir( int fd, int index, struct dirent* entry )
{
    if( FD_CHECK(fd) )
    {
        PTR_VALIDATE(entry);
        struct dirent* kentry = readdir_fs(FD_ENTRY(fd), (uint32_t)index);

        if(kentry)
        {
            memcpy(entry, kentry, sizeof *entry);
            free(kentry);
            return 1;
        }else
        {
            return 0;
        }
    }

    return -EBADF;
}

static int sys_chdir( char* dirpath )
{
    PTR_VALIDATE(dirpath);

    char* path = canonicalize_path(current_process->wd_name, dirpath);
    fs_node_t* chd = kopen(path, 0);
    if( chd )
    {
        if( (chd->type & FS_DIRECTORY) == 0 )
        {
            close_fs(chd);
            return -ENOTDIR;
        }
        if( !has_permission(chd, 01) )
        {
            close_fs(chd);
            return -EACCES;
        }
        close_fs(chd);
        free(current_process->wd_name);
        current_process->wd_name = malloc(strlen(path) + 1);
        memcpy(current_process->wd_name, path, strlen(path) + 1);
        return 0;
    }else
    {
        return -ENOENT;
    }
}

static int sys_getcwd( char* buf, size_t size )
{
    if( buf )
    {
        PTR_VALIDATE(buf);
        size_t len = strlen(current_process->wd_name) + 1;
        return (int)memcpy(buf, current_process->wd_name, MIN(size, len));
    }

    return 0;
}

static int sys_getgid( void )
{
    return current_process->user_group;
}

static int sys_setuid( user_t new_uid )
{
    if( current_process->user == USER_ROOT_UID )
    {
        current_process->user = new_uid;
        current_process->real_user = new_uid;
        return 0;
    }

    return -EPERM;
}

static int sys_sleepabs( unsigned long seconds, unsigned long subseconds )
{
    sleep_until((process_t*)current_process, seconds, subseconds);

    switch_task(0);

    if( seconds > timer_ticks || (seconds == timer_ticks && subseconds >= timer_subticks) )
    {
        return 0;
    }else
    {
        return 1;
    }
}

static int sys_sleep( unsigned long seconds, unsigned long subseconds )
{
    unsigned long s, ss;
    relative_time(seconds, subseconds * 10, &s, &ss);

    return sys_sleepabs(s, ss);
}

static int sys_ioctl( int fd, int request, void* argp )
{
    if( FD_CHECK(fd) )
    {
        PTR_VALIDATE(argp);
        return ioctl_fs(FD_ENTRY(fd), request, argp);
    }

    return -EBADF;
}

static int sys_access( const char* file, int flags )
{
    PTR_VALIDATE(file);
    
    fs_node_t* node = kopen((char*)file, flags);

    if( !node ) return -ENOENT;

    close_fs(node);

    return 0;
}

static int sys_yield( void )
{
    switch_task(1);

    return 1;
}

static int sys_fswait( int c, int fds[] )
{
    PTR_VALIDATE(fds);
    
    for( int i = 0; i < c; i++ )
    {
        if( !FD_CHECK(fds[i]) )
        {
            return -EBADF;
        }
    }

    fs_node_t** nodes = malloc(sizeof(fs_node_t*)*(c + 1));

    for( int i = 0; i < c; i++ )
    {
        nodes[i] = FD_ENTRY(fds[i]);
    }
    nodes[c] = NULL;

    int result = process_wait_nodes((process_t*)current_process, nodes, -1);
    free(nodes);

    return result;
}

static int sys_fswait2( int c, int fds[], int timeout )
{
    PTR_VALIDATE(fds);
    
    for( int i = 0; i < c; i++ )
    {
        if( !FD_CHECK(fds[i]) )
        {
            return -EBADF;
        }
    }

    fs_node_t** nodes = malloc(sizeof(fs_node_t*)*(c + 1));

    for( int i = 0; i < c; i++ )
    {
        nodes[i] = FD_ENTRY(fds[i]);
    }
    nodes[c] = NULL;

    int result = process_wait_nodes((process_t*)current_process, nodes, timeout);
    free(nodes);
    return result;
}

static int sys_fswait3( int c, int fds[], int timeout, int out[] )
{
    PTR_VALIDATE(fds);
    PTR_VALIDATE(out);

    int has_match = -1;
    for( int i = 0; i < c; i++ )
    {
        if( !FD_CHECK(fds[i]) )
        {
            return -EBADF;
        }
        if( selectcheck_fs(FD_ENTRY(fds[i])) == 0 )
        {
            out[i] = 1;
            has_match = (has_match == -1) ? i : has_match;
        }else
        {
            out[i] = 0;
        }
    }

    /* Already found a match, return immediately with the first match */
    if( has_match != -1 )
    {
        return has_match;
    }

    int result = sys_fswait2(c, fds, timeout);
    if( result != -1 )
    {
        out[result] = 1;
    }
    
    return result;
}

static int sys_waitpid( int pid, int* status, int options )
{
    if( status && !PTR_INRANGE(status) )
    {
        return -EINVAL;
    }

    return waitpid(pid, status, options);
}

static int sys_umask( int mode )
{
    current_process->mask = mode & 0777;

    return 0;
}

static int sys_chmod( char* file, int mode )
{
    int result;
    PTR_VALIDATE(file);

    fs_node_t* fn = kopen(file, 0);

    if( fn )
    {
        if( current_process->user != 0 && current_process->user != fn->uid )
        {
            close_fs(fn);
            return -EACCES;
        }
        result = chmod_fs(fn, mode);
        close_fs(fn);

        return result;
    }else
    {
        return -ENOENT;
    }
}

static int sys_unlink( char* file )
{
    PTR_VALIDATE(file);

    return unlink_fs(file);
}

static int sys_pipe( int fd[2] )
{
    if( fd && !PTR_INRANGE(fd) )
    {
        return -EFAULT;
    }

    fs_node_t* outpipes[2];

    make_unix_pipe(outpipes);

    open_fs(outpipes[0], 0);
    open_fs(outpipes[1], 0);

    fd[0] = process_append_fd((process_t*)current_process, outpipes[0]);
    fd[1] = process_append_fd((process_t*)current_process, outpipes[1]);
    FD_FLAG(fd[0]) = FS_O_RDWR;
    FD_FLAG(fd[1]) = FS_O_RDWR;

    return 0;
}

static int sys_mount( char* arg, char* mountpoint, char* type )
{
    PTR_VALIDATE(arg);
    PTR_VALIDATE(mountpoint);
    PTR_VALIDATE(type);

    if( current_process->user != USER_ROOT_UID )
    {
        return -EPERM;
    }

    if( PTR_INRANGE(arg) && PTR_INRANGE(mountpoint) && PTR_INRANGE(type) )
    {
        return vfs_mount_type(type, arg, mountpoint);
    }

    return -EFAULT;
}

static int sys_symlink( char* target, char* name )
{
    PTR_VALIDATE(target);
    PTR_VALIDATE(name);

    return symlink_fs(target, name);
}

static int sys_chown( char* file, int uid, int gid )
{
    int result;
    PTR_VALIDATE(file);

    fs_node_t* fn = kopen(file, 0);

    if( fn )
    {
        if( current_process->user != 0 || current_process->user != fn->uid )
        {
            close_fs(fn);
            return -EACCES;
        }
        result = chown_fs(fn, uid, gid);
        close_fs(fn);

        return result;
    }else
    {
        return -ENOENT;
    }
}

static int sys_setsid( void )
{
    if( current_process->job == current_process->group )
    {
        return -EPERM;
    }

    current_process->session = current_process->group;
    current_process->job = current_process->group;

    return current_process->session;
}

static int sys_setpgid( pid_t pid, pid_t pgid )
{
    if( pgid < 0 )
    {
        return -EINVAL;
    }

    process_t* proc;

    if( pid == 0 )
    {
        proc = (process_t*)current_process;
    }else
    {
        proc = process_from_pid(pid);
    }

    if( !proc )
    {
        debug_log("setpgid - process not found");
        return -ESRCH;
    }

    if( proc->session != current_process->session )
    {
        debug_log("setpgid - child is in different session");
        return -EPERM;
    }

    if( proc->session == proc->group )
    {
        debug_log("setpgid - process is session leader");
        return -EPERM;
    }

    if( pgid == 0 )
    {
        proc->job = proc->group;
    }else
    {
        process_t* pgroup = process_from_pid(pgid);

        if( !pgroup )
        {
            debug_log("setpgid - can't find pgroup");
            return -EPERM;
        }

        if( pgroup->session != proc->session )
        {
            debug_log("setpgid - tried to move to a different session");
            return -EPERM;
        }

        proc->job = pgid;
    }

    return 0;
}

static int sys_getpgid( pid_t pid )
{
    process_t* proc;

    if( pid == 0 )
    {
        proc = (process_t*)current_process;
    }else
    {
        proc = process_from_pid(pid);
    }

    if( !proc )
    {
        debug_log("getpgid - process not found");
        return -ESRCH;
    }

    return proc->job;
}

static int sys_mmap( uintptr_t address, size_t size )
{
    if( address < 0x10000000 )
    {
        return -EINVAL;
    }

    if( address & 0xFFF )
    {
        size += address & 0xFFF;
        address &= 0xFFFFF000;
    }

    process_t* proc = (process_t*)current_process;
    if( proc->group != 0 )
    {
        proc = process_from_pid(proc->group);
    }

    spin_lock(proc->image.lock);
    for( size_t x = 0; x < size; x += 0x1000 )
    {
        alloc_frame(get_page(address + x, 1, current_directory), 0, 1);
        invalidate_tables_at(address + x);
    }
    spin_unlock(proc->image.lock);

    return 0;
}

static int sys_getppid( void )
{
    return process_get_parent((process_t*)current_process)->id;
}

static int sys_fcntl( int fd, int cmd, va_list args )
{
    if( !FD_CHECK(fd) )
    {
        return -EBADF;
    }

    int flag = FD_FLAG(fd);

    switch(cmd)
    {
        case F_GETFD:
            return flag >> 16;
        case F_SETFD:
            flag = (flag & ~(0xF0000)) | (va_arg(args, int) >> 16);
            break;
        case F_GETFL:
            return flag & 0xFFFF;
        case F_SETFL:
            flag = (flag & (0xF000F)) | (va_arg(args, int) & 0xFFF0);
            break;
        default:
            return -EINVAL;
    }

    FD_FLAG(fd) = flag;
    return 0;
}

static int sys_debugvfstree( char** str )
{
    debug_print_vfs_tree(str);

    return 0;
}

static int sys_debugproctree( char** str )
{
    debug_print_proc_tree(str);
    
    return 0;
}

static int sys_debugprint( char* message )
{
    debug_log(message);

    return 0;
}

#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wstrict-prototypes"
#pragma GCC diagnostic ignored "-Wincompatible-pointer-types"
static int (*syscalls[])() =
{
    [SYS_EXT] = sys_exit,
    [SYS_OPEN] = sys_open,
    [SYS_READ] = sys_read,
    [SYS_WRITE] = sys_write,
    [SYS_CLOSE] = sys_close,
    [SYS_GETTIMEOFDAY] = sys_gettimeofday,
    [SYS_EXECVE] = sys_execve,
    [SYS_FORK] = sys_fork,
    [SYS_CLONE] = sys_clone,
    [SYS_GETPID] = sys_getpid,
    [SYS_SBRK] = sys_sbrk,
    [SYS_UNAME] = sys_uname,
    [SYS_SETHOSTNAME] = sys_sethostname,
    [SYS_GETHOSTNAME] = sys_gethostname,
    [SYS_MKDIR] = sys_mkdir,
    [SYS_KILL] = sys_kill,
    [SYS_SIGNAL] = sys_signal,
    [SYS_GETTID] = sys_gettid,
    [SYS_OPENPTY] = sys_openpty,
    [SYS_SEEK] = sys_seek,
    [SYS_READLINK] = sys_readlink,
    [SYS_STAT] = sys_stat,
    [SYS_STATF] = sys_statf,
    [SYS_LSTAT] = sys_lstat,
    [SYS_DUP2] = sys_dup2,
    [SYS_GETUID] = sys_getuid,
    [SYS_REBOOT] = sys_reboot,
    [SYS_READDIR] = sys_readdir,
    [SYS_CHDIR] = sys_chdir,
    [SYS_GETCWD] = sys_getcwd,
    [SYS_GETGID] = sys_getgid,
    [SYS_SETUID] = sys_setuid,
    [SYS_SLEEPABS] = sys_sleepabs,
    [SYS_SLEEP] = sys_sleep,
    [SYS_IOCTL] = sys_ioctl,
    [SYS_ACCESS] = sys_access,
    [SYS_YIELD] = sys_yield,
    [SYS_FSWAIT] = sys_fswait,
    [SYS_FSWAIT2] = sys_fswait2,
    [SYS_FSWAIT3] = sys_fswait3,
    [SYS_WAITPID] = sys_waitpid,
    [SYS_UMASK] = sys_umask,
    [SYS_CHMOD] = sys_chmod,
    [SYS_UNLINK] = sys_unlink,
    [SYS_PIPE] = sys_pipe,
    [SYS_MOUNT] = sys_mount,
    [SYS_SYMLINK] = sys_symlink,
    [SYS_CHOWN] = sys_chown,
    [SYS_SETSID] = sys_setsid,
    [SYS_SETPGID] = sys_setpgid,
    [SYS_GETPGID] = sys_getpgid,
    [SYS_MMAP] = sys_mmap,
    [SYS_GETPPID] = sys_getppid,
    [SYS_FCNTL] = sys_fcntl,
    [SYS_DEBUGVFSTREE] = sys_debugvfstree,
    [SYS_DEBUGPROCTREE] = sys_debugproctree,
    [SYS_DEBUGPRINT] = sys_debugprint,
};
#pragma GCC diagnostic pop

static void syscall_handler( struct regs* r )
{
    uintptr_t location = (uintptr_t)syscalls[r->eax];
    if( !location )
        return;

    current_process->syscall_registers = r;

    syscall_function_t syscall_function = (syscall_function_t)location;
    uint32_t ret = syscall_function(r->ebx, r->ecx, r->edx, r->esi, r->edi);

    if( (current_process->syscall_registers == r) || (location != (uintptr_t)&fork && location != (uintptr_t)&clone) )
    {
        r->eax = ret;
    }
}

void syscalls_initialize( void )
{
    isr_install_handler(SYSCALL_VECTOR, &syscall_handler);
}
