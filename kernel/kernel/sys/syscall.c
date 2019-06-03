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

static int sys_yield( void )
{
    switch_task(1);

    return 1;
}

static int (*syscalls[])() =
{
    [SYS_EXT] = sys_exit,
    [SYS_OPEN] = sys_open,
    [SYS_READ] = sys_read,
    [SYS_WRITE] = sys_write,
    [SYS_CLOSE] = sys_close,
    [SYS_SBRK] = sys_sbrk,
    [SYS_SLEEPABS] = sys_sleepabs,
    [SYS_SLEEP] = sys_sleep,
    [SYS_YIELD] = sys_yield,
};

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
