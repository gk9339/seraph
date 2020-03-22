#include <kernel/irq.h>
#include <kernel/timer.h>
#include <kernel/isr.h>
#include <sys/types.h>
#include <kernel/types.h>
#include <kernel/process.h>
#include <kernel/task.h>
#include <kernel/fs.h>
#include <kernel/serial.h>
#include <kernel/syscall.h>
#include <kernel/spinlock.h>
#include <sys/syscall.h>
#include <errno.h>
#include <stdlib.h>
#include <kernel/elf.h>
#include <kernel/shm.h>
#include <kernel/pty.h>

#define FD_INRANGE(FD) ((FD) < (int)current_process->fds->length && (FD) >= 0)
#define FD_ENTRY(FD) (current_process->fds->entries[(FD)])
#define FD_CHECK(FD) (FD_INRANGE(FD) && FD_ENTRY(FD))
#define FD_OFFSET(FD) (current_process->fds->offsets[(FD)])
#define FD_MODE(FD) (current_process->fds->modes[(FD)])

#define PTR_INRANGE(PTR) ((uintptr_t)(PTR) > current_process->image.entry)
#define PTR_VALIDATE(PTR) ptr_validate((void *)(PTR), __func__)

static void ptr_validate( void* ptr, const char* syscall )
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

    int access_bits = 0;

    if( node && (flags & O_CREAT) && (flags & O_EXCL) )
    {
        close_fs(node);
        return -EEXIST;
    }

    if( !(flags & O_WRONLY) || (flags & O_RDWR) )
    {
        if( node && !has_permission(node, 04) )
        {
            close_fs(node);
            return -EACCES;
        }else
        {
            access_bits |= 01;
        }
    }

    if( (flags & O_RDWR) || (flags & O_WRONLY) )
    {
        if( node && !has_permission(node, 02) )
        {
            close_fs(node);
            return -EACCES;
        }
        
        if( node &&  (node->flags & FS_DIRECTORY) )
        {
            return -EISDIR;
        }

        if( (flags & O_RDWR) || (flags & O_WRONLY) )
        {
            access_bits |= 02;
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
        if( !(node->flags & FS_DIRECTORY) )
        {
            return -ENOTDIR;
        }
    }

    if( node && (flags & O_TRUNC) )
    {
        if( !(access_bits & 02) )
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

    if( node && (flags & O_CREAT) && (node->flags & FS_DIRECTORY) )
    {
        close_fs(node);
        return -EISDIR;
    }

    int fd = process_append_fd((process_t*)current_process, node);
    FD_MODE(fd) = access_bits;
    if( flags & O_APPEND )
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
        if( !(FD_MODE(fd) & 01) )
        {
            return -EACCES;
        }

        uint32_t out = read_fs(node, (uint32_t)FD_OFFSET(fd), len, (uint8_t*)ptr);
        FD_OFFSET(fd) += out;

        return (int)out;
    }

    return -EBADF;
}

static int sys_write( int fd, char* ptr, int len )
{
    if( FD_CHECK(fd) )
    {
        PTR_VALIDATE(ptr);
        fs_node_t* node = FD_ENTRY(fd);
        if( !(FD_MODE(fd) & 02) )
        {
            return -EACCES;
        }

        uint32_t out = write_fs(node, (uint32_t)FD_OFFSET(fd), len, (uint8_t*)ptr);
        FD_OFFSET(fd) += out;

        return out;
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

    FD_MODE(*master) = 03;
    FD_MODE(*slave) = 03;

    open_fs(fs_master, 0);
    open_fs(fs_slave, 0);

    return 0;
}

static int sys_seek( int fd, int offset, int whence )
{
    if( FD_CHECK(fd) )
    {
        if( (FD_ENTRY(fd)->flags & FS_PIPE) || (FD_ENTRY(fd)->flags & FS_CHARDEVICE) )
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

static int stat_node( fs_node_t* fn, uintptr_t st )
{
    struct stat* f = (struct stat*)st;
    PTR_VALIDATE(f);

    if( !fn )
    {
        memset(f, 0, sizeof(struct stat));
        debug_log("stat: file does not exist");
        return -ENOENT;
    }

    f->st_dev = (uint16_t)(((uint32_t)fn->device * 0xFFFF0) >> 8);
    f->st_ino = fn->inode;

    uint32_t flags = 0;
    if( fn->flags & FS_FILE )
    {
        flags |= _IFREG;
    }
    if( fn->flags & FS_DIRECTORY )
    {
        flags |= _IFDIR;
    }
    if( fn->flags & FS_CHARDEVICE )
    {
        flags |= _IFCHR;
    }
    if( fn->flags & FS_BLOCKDEVICE )
    {
        flags |= _IFBLK;
    }
    if( fn->flags & FS_PIPE )
    {
        flags |= _IFIFO;
    }
    if( fn->flags & FS_SYMLINK )
    {
        flags |= _IFLNK;
    }

    f->st_mode = fn->mask | flags;
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

    fs_node_t* fn = kopen(file, O_PATH | O_NOFOLLOW);
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

static int sys_waitpid( int pid, int* status, int options )
{
    if( status && !PTR_INRANGE(status) )
    {
        return -EINVAL;
    }

    return waitpid(pid, status, options);
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

static int sys_debugvfstree( void )
{
    debug_print_vfs_tree();

    return 0;
}

static int sys_debugproctree( void )
{
    debug_print_proc_tree();
    
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
    [SYS_EXECVE] = sys_execve,
    [SYS_FORK] = sys_fork,
    [SYS_GETPID] = sys_getpid,
    [SYS_SBRK] = sys_sbrk,
    [SYS_SIGNAL] = sys_signal,
    [SYS_OPENPTY] = sys_openpty,
    [SYS_SEEK] = sys_seek,
    [SYS_STATF] = sys_statf,
    [SYS_LSTAT] = sys_lstat,
    [SYS_DUP2] = sys_dup2,
    [SYS_GETUID] = sys_getuid,
    [SYS_SETUID] = sys_setuid,
    [SYS_SLEEPABS] = sys_sleepabs,
    [SYS_SLEEP] = sys_sleep,
    [SYS_IOCTL] = sys_ioctl,
    [SYS_YIELD] = sys_yield,
    [SYS_FSWAIT] = sys_fswait,
    [SYS_FSWAIT2] = sys_fswait2,
    [SYS_WAITPID] = sys_waitpid,
    [SYS_SETSID] = sys_setsid,
    [SYS_SETPGID] = sys_setpgid,
    [SYS_GETPGID] = sys_getpgid,
    [SYS_DEBUGVFSTREE] = sys_debugvfstree,
    [SYS_DEBUGPROCTREE] = sys_debugproctree,
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
