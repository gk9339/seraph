#include <kernel/fs.h>
#include <kernel/process.h>
#include <kernel/multiboot.h>
#include <kernel/version.h>
#include <kernel/kernel.h>
#include <stdio.h>
#include <stdlib.h>
#include <kernel/serial.h>
#include <kernel/cmos.h>
#include <kernel/mem.h>
#include <kernel/shm.h>
#include <kernel/timer.h>
#include <kernel/irq.h>

struct procfs_entry
{
    int         id;
    char*       name;
    read_type_t func;
};

#define PROCFS_STANDARD_ENTRIES (sizeof(std_entries) / sizeof(struct procfs_entry))
#define PROCFS_PROCDIR_ENTRIES  (sizeof(procdir_entries) / sizeof(struct procfs_entry))

static fs_node_t* procfs_generic_create( char* name, read_type_t read_func )
{
    fs_node_t* fnode = malloc(sizeof(fs_node_t));
    memset(fnode, 0x00, sizeof(fs_node_t));
    fnode->inode = 0;
    strcpy(fnode->name, name);
    fnode->uid = 0;
    fnode->gid = 0;
    fnode->mask    = 0444;
    fnode->type   = FS_FILE;
    fnode->read    = read_func;
    fnode->write   = NULL;
    fnode->open    = NULL;
    fnode->close   = NULL;
    fnode->readdir = NULL;
    fnode->finddir = NULL;
    fnode->ctime   = now();
    fnode->mtime   = now();
    fnode->atime   = now();
    return fnode;
}

static uint32_t proc_cmdline_func(fs_node_t* node, uint32_t offset, uint32_t size, uint8_t* buffer )
{
    char buf[1024];
    process_t* proc = process_from_pid(node->inode);

    if( !proc )
    {
        return 0;
    }

    if( !proc->cmdline )
    {
        sprintf(buf, "%s", proc->name);

        size_t _bsize = strlen(buf);
        if( offset > _bsize ) return 0;
        if( size > _bsize - offset ) size = _bsize - offset;

        memcpy(buffer, buf + offset, size);
        return size;
    }


    buf[0] = '\0';

    char*  _buf = buf;
    char** args = proc->cmdline;
    while( *args )
    {
        strcpy(_buf, *args);
        _buf += strlen(_buf);
        if( *(args+1) )
        {
            strcpy(_buf, "\036");
            _buf += strlen(_buf);
        }
        args++;
    }

    size_t _bsize = strlen(buf);
    if( offset > _bsize ) return 0;
    if( size > _bsize - offset ) size = _bsize - offset;

    memcpy(buffer, buf + offset, size);
    return size;
}

static size_t calculate_memory_usage( page_directory_t* src )
{
    size_t pages = 0;
    for( uint32_t i = 0; i < 1024; ++i )
    {
        if( !src->tables[i] || (uintptr_t)src->tables[i] == (uintptr_t)0xFFFFFFFF )
        {
            continue;
        }
        if( kernel_directory->tables[i] == src->tables[i] )
        {
            continue;
        }
        // For each table
        if( i * 0x1000 * 1024 < SHM_START )
        {
            // Ignore shared memory for now
            for( int j = 0; j < 1024; ++j )
            {
                // For each frame in the table...
                if( !src->tables[i]->pages[j].frame )
                {
                    continue;
                }
                pages++;
            }
        }
    }
    return pages;
}

static size_t calculate_shm_resident( page_directory_t* src )
{
    size_t pages = 0;
    for( uint32_t i = 0; i < 1024; ++i )
    {
        if( !src->tables[i] || (uintptr_t)src->tables[i] == (uintptr_t)0xFFFFFFFF )
        {
            continue;
        }
        if( kernel_directory->tables[i] == src->tables[i] )
        {
            continue;
        }
        if( i * 0x1000 * 1024 < SHM_START )
        {
            continue;
        }
        for( int j = 0; j < 1024; ++j )
        {
            if( !src->tables[i]->pages[j].frame )
            {
                continue;
            }
            pages++;
        }
    }
    return pages;
}

static uint32_t proc_status_func( fs_node_t* node, uint32_t offset, uint32_t size, uint8_t* buffer )
{
    char buf[2048];
    process_t* proc = process_from_pid(node->inode);
    process_t* parent = process_get_parent(proc);

    if( !proc )
    {
        return 0;
    }

    char* state = proc->finished ? "F (finished)" : (proc->suspended ? "S (suspended)" : (proc->running ? "R (running)" : "S (sleeping)"));
    char* name = proc->name + strlen(proc->name) - 1;

    while( 1 )
    {
        if( *name == '/' )
        {
            name++;
            break;
        }
        if( name == proc->name ) break;
        name--;
    }

    // Calculate process memory usage
    int mem_usage = calculate_memory_usage(proc->thread.page_directory) * 4;
    int shm_usage = calculate_shm_resident(proc->thread.page_directory) * 4;
    int mem_permille = 1000 * (mem_usage + shm_usage) / memory_total();

    sprintf(buf,
            "Name:\t%s\n" // name
            "State:\t%s\n" // yeah, do this at some point
            "Tgid:\t%d\n" // group ? group : pid
            "Pid:\t%d\n" // pid
            "PPid:\t%d\n" // parent pid
            "Pgid:\t%d\n" // progress group id
            "Sid:\t%d\n" // session id
            "Uid:\t%d\n"
            "Ueip:\t0x%x\n"
            "SCid:\t%d\n"
            "SC0:\t0x%x\n"
            "SC1:\t0x%x\n"
            "SC2:\t0x%x\n"
            "SC3:\t0x%x\n"
            "SC4:\t0x%x\n"
            "UserStack:\t0x%x\n"
            "Path:\t%s\n"
            "VmSize:\t %d kB\n"
            "RssShmem:\t %d kB\n"
            "MemPermille:\t %d\n",
            name,
            state,
            proc->group ? proc->group : proc->id,
            proc->id,
            parent ? parent->id : 0,
            proc->job,
            proc->session,
            proc->user,
            proc->syscall_registers ? proc->syscall_registers->eip : 0,
            proc->syscall_registers ? proc->syscall_registers->eax : 0,
            proc->syscall_registers ? proc->syscall_registers->ebx : 0,
            proc->syscall_registers ? proc->syscall_registers->ecx : 0,
            proc->syscall_registers ? proc->syscall_registers->edx : 0,
            proc->syscall_registers ? proc->syscall_registers->esi : 0,
            proc->syscall_registers ? proc->syscall_registers->edi : 0,
            proc->syscall_registers ? proc->syscall_registers->useresp : 0,
            proc->cmdline ? proc->cmdline[0] : "(none)",
            mem_usage, shm_usage, mem_permille);

    size_t _bsize = strlen(buf);
    if( offset > _bsize ) return 0;
    if( size > _bsize - offset ) size = _bsize - offset;

    memcpy(buffer, buf + offset, size);
    return size;
}

static struct procfs_entry procdir_entries[] =
{
    {1, "cmdline", proc_cmdline_func},
    {2, "status",  proc_status_func},
};

static struct dirent* readdir_procfs_procdir( fs_node_t* node __attribute__((unused)), uint32_t index )
{
    if( index == 0 )
    {
        struct dirent* out = malloc(sizeof(struct dirent));
        memset(out, 0x00, sizeof(struct dirent));
        out->ino = 0;
        strcpy(out->name, ".");
        return out;
    }

    if( index == 1 )
    {
        struct dirent* out = malloc(sizeof(struct dirent));
        memset(out, 0x00, sizeof(struct dirent));
        out->ino = 0;
        strcpy(out->name, "..");
        return out;
    }

    index -= 2;

    if( index < PROCFS_PROCDIR_ENTRIES )
    {
        struct dirent* out = malloc(sizeof(struct dirent));
        memset(out, 0x00, sizeof(struct dirent));
        out->ino = procdir_entries[index].id;
        strcpy(out->name, procdir_entries[index].name);
        return out;
    }

    return NULL;
}

static fs_node_t* finddir_procfs_procdir( fs_node_t* node, char* name )
{
    if( !name ) return NULL;

    for( unsigned int i = 0; i < PROCFS_PROCDIR_ENTRIES; ++i )
    {
        if( !strcmp(name, procdir_entries[i].name) )
        {
            fs_node_t* out = procfs_generic_create(procdir_entries[i].name, procdir_entries[i].func);
            out->inode = node->inode;
            return out;
        }
    }

    return NULL;
}

static fs_node_t* procfs_procdir_create( process_t* process )
{
    pid_t pid = process->id;
    fs_node_t* fnode = malloc(sizeof(fs_node_t));
    memset(fnode, 0x00, sizeof(fs_node_t));
    fnode->inode = pid;
    sprintf(fnode->name, "%d", pid);
    fnode->uid = 0;
    fnode->gid = 0;
    fnode->mask = 0555;
    fnode->type   = FS_DIRECTORY;
    fnode->read    = NULL;
    fnode->write   = NULL;
    fnode->open    = NULL;
    fnode->close   = NULL;
    fnode->readdir = readdir_procfs_procdir;
    fnode->finddir = finddir_procfs_procdir;
    fnode->nlink   = 1;
    fnode->ctime   = process->start.tv_sec;
    fnode->mtime   = process->start.tv_sec;
    fnode->atime   = process->start.tv_sec;
    return fnode;
}

static uint32_t meminfo_func( fs_node_t* node __attribute__((unused)), uint32_t offset, uint32_t size, uint8_t* buffer )
{
    char buf[1024];
    unsigned int total = memory_total();
    unsigned int free  = total - memory_use();
    unsigned int kheap = (heap_end - placement_pointer) / 1024;

    sprintf(buf,
        "MemTotal: %d kB\n"
        "MemFree: %d kB\n"
        "KHeapUse: %d kB\n", total, free, kheap);

    size_t _bsize = strlen(buf);
    if( offset > _bsize ) return 0;
    if( size > _bsize - offset ) size = _bsize - offset;

    memcpy(buffer, buf + offset, size);
    return size;
}

static uint32_t uptime_func( fs_node_t* node __attribute__((unused)), uint32_t offset, uint32_t size, uint8_t* buffer )
{
    char buf[1024];
    sprintf(buf, "%d.%3d\n", timer_ticks, timer_subticks);

    size_t _bsize = strlen(buf);
    if( offset > _bsize ) return 0;
    if( size > _bsize - offset ) size = _bsize - offset;

    memcpy(buffer, buf + offset, size);
    return size;
}


static uint32_t cmdline_func( fs_node_t* node __attribute__((unused)), uint32_t offset, uint32_t size, uint8_t* buffer )
{
    char buf[1024];
    sprintf(buf, "%s\n", kcmdline);

    size_t _bsize = strlen(buf);
    if( offset > _bsize ) return 0;
    if( size > _bsize - offset ) size = _bsize - offset;

    memcpy(buffer, buf + offset, size);
    return size;
}

static uint32_t version_func( fs_node_t* node __attribute__((unused)), uint32_t offset, uint32_t size, uint8_t* buffer )
{
    char buf[1024];
    char version_number[512];
    sprintf(version_number, "%d.%d.%d",
            _kernel_version_major,
            _kernel_version_minor,
            _kernel_version_lower);
    sprintf(buf, "%s %s %s %s %s %s\n",
            _kernel_name,
            version_number,
            _kernel_build_date,
            _kernel_build_time,
            _kernel_build_timezone,
            _kernel_arch);

    size_t _bsize = strlen(buf);
    if( offset > _bsize ) return 0;
    if( size > _bsize - offset ) size = _bsize - offset;

    memcpy(buffer, buf + offset, size);
    return size;
}

static void mount_recurse( char* buf, tree_node_t* node, size_t height )
{
    // End recursion on a blank entry
    if( !node ) return;
    char* tmp = malloc(512);
    memset(tmp, 0, 512);
    char* c = tmp;
    // Indent output
    for( uint32_t i = 0; i < height; ++i )
    {
        c += sprintf(c, "  ");
    }
    // Get the current process
    struct vfs_entry* fnode = (struct vfs_entry *)node->value;
    // Print the process name
    if( fnode->file )
    {
        c += sprintf(c, "%s → %s 0x%x (%s, %s)", fnode->name, fnode->device, fnode->file, fnode->fs_type, fnode->file->name);
    }else
    {
        c += sprintf(c, "%s → (empty)", fnode->name);
    }
    // Linefeed
    sprintf(buf+strlen(buf),"%s\n",tmp);
    free(tmp);
    foreach(child, node->children)
    {
        // Recursively print the children
        mount_recurse(buf+strlen(buf),child->value, height + 1);
    }
}

static uint32_t mounts_func( fs_node_t* node __attribute__((unused)), uint32_t offset, uint32_t size, uint8_t* buffer )
{
    char* buf = malloc(4096);

    buf[0] = '\0';

    mount_recurse(buf, fs_tree->root, 0);

    size_t _bsize = strlen(buf);
    if( offset > _bsize )
    {
        free(buf);
        return 0;
    }
    if( size > _bsize - offset ) size = _bsize - offset;

    memcpy(buffer, buf + offset, size);
    free(buf);
    return size;
}

static uint32_t filesystems_func( fs_node_t* node __attribute__((unused)), uint32_t offset, uint32_t size, uint8_t* buffer )
{
    list_t* hash_keys = hashtable_keys(fs_types);
    char* buf = malloc(hash_keys->length * 512);
    unsigned int soffset = 0;
    foreach(_key, hash_keys)
    {
        char* key = (char*)_key->value;
        soffset += sprintf(&buf[soffset], "%s\n", key);
    }
    free(hash_keys);

    size_t _bsize = strlen(buf);
    if( offset > _bsize )
    {
        free(buf);
        return 0;
    }
    if( size > _bsize - offset ) size = _bsize - offset;

    memcpy(buffer, buf + offset, size);
    free(buf);
    return size;
}

static uint32_t loader_func( fs_node_t* node __attribute__((unused)), uint32_t offset, uint32_t size, uint8_t* buffer )
{
    char* buf = malloc(512);

    if( mbi->flags & MULTIBOOT_INFO_BOOT_LOADER_NAME )
    {
        sprintf(buf, "%s\n", mbi->boot_loader_name);
    }else
    {
        buf[0] = '\n';
        buf[1] = '\0';
    }

    size_t _bsize = strlen(buf);
    if( offset > _bsize )
    {
        free(buf);
        return 0;
    }
    if( size > _bsize - offset ) size = _bsize - offset;

    memcpy(buffer, buf + offset, size);
    free(buf);
    return size;
}

static uint32_t irq_func( fs_node_t* node __attribute__((unused)), uint32_t offset, uint32_t size, uint8_t* buffer )
{
    char* buf = malloc(4096);
    unsigned int soffset = 0;

    for( int i = 0; i < 16; ++i )
    {
        soffset += sprintf(&buf[soffset], "irq %d: ", i);
        for( int j = 0; j < 4; ++j )
        {
            char* t = get_irq_handler(i, j);
            if( !t ) break;
            soffset += sprintf(&buf[soffset], "%s%s", j ? "," : "", t);
        }
        soffset += sprintf(&buf[soffset], "\n");
    }

    size_t _bsize = strlen(buf);
    if( offset > _bsize )
    {
        free(buf);
        return 0;
    }
    if( size > _bsize - offset ) size = _bsize - offset;

    memcpy(buffer, buf + offset, size);
    free(buf);
    return size;
}

static struct procfs_entry std_entries[] =
{
    {-1, "cpuinfo",  cpuinfo_func},
    {-2, "meminfo",  meminfo_func},
    {-3, "uptime",   uptime_func},
    {-4, "cmdline",  cmdline_func},
    {-5, "version",  version_func},
    {-6, "mounts",   mounts_func},
    {-7, "filesystems", filesystems_func},
    {-8, "loader",   loader_func},
    {-9, "irq",      irq_func},
};

static struct dirent* readdir_procfs_root( fs_node_t* node __attribute__((unused)), uint32_t index )
{
    if( index == 0 )
    {
        struct dirent* out = malloc(sizeof(struct dirent));
        memset(out, 0x00, sizeof(struct dirent));
        out->ino = 0;
        strcpy(out->name, ".");
        return out;
    }

    if( index == 1 )
    {
        struct dirent* out = malloc(sizeof(struct dirent));
        memset(out, 0x00, sizeof(struct dirent));
        out->ino = 0;
        strcpy(out->name, "..");
        return out;
    }

    if( index == 2 )
    {
        struct dirent* out = malloc(sizeof(struct dirent));
        memset(out, 0x00, sizeof(struct dirent));
        out->ino = 0;
        strcpy(out->name, "self");
        return out;
    }

    index -= 3;

    if( index < PROCFS_STANDARD_ENTRIES )
    {
        struct dirent* out = malloc(sizeof(struct dirent));
        memset(out, 0x00, sizeof(struct dirent));
        out->ino = std_entries[index].id;
        strcpy(out->name, std_entries[index].name);
        return out;
    }

    index -= PROCFS_STANDARD_ENTRIES;

    int i = index + 1;

    pid_t pid = 0;

    foreach(lnode, process_list)
    {
        i--;
        if( i == 0 )
        {
            process_t* proc = (process_t *)lnode->value;
            pid = proc->id;
            break;
        }
    }

    if( pid == 0 )
    {
        return NULL;
    }

    struct dirent* out = malloc(sizeof(struct dirent));
    memset(out, 0x00, sizeof(struct dirent));
    out->ino  = pid;
    sprintf(out->name, "%d", pid);

    return out;
}


static int readlink_self( fs_node_t* node __attribute__((unused)), char* buf, size_t size )
{
    char tmp[30];
    size_t req;
    sprintf(tmp, "/proc/%d", current_process->id);
    req = strlen(tmp) + 1;

    if( size < req )
    {
        memcpy(buf, tmp, size);
        buf[size-1] = '\0';
        return size-1;
    }

    if( size > req ) size = req;

    memcpy(buf, tmp, size);
    return size-1;
}

static fs_node_t* procfs_create_self( void )
{
    fs_node_t* fnode = malloc(sizeof(fs_node_t));
    memset(fnode, 0x00, sizeof(fs_node_t));
    fnode->inode = 0;
    strcpy(fnode->name, "self");
    fnode->mask = 0777;
    fnode->uid  = 0;
    fnode->gid  = 0;
    fnode->type   = FS_FILE | FS_SYMLINK;
    fnode->readlink = readlink_self;
    fnode->nlink   = 1;
    fnode->ctime   = now();
    fnode->mtime   = now();
    fnode->atime   = now();
    return fnode;
}

static fs_node_t* finddir_procfs_root( fs_node_t* node __attribute__((unused)), char* name )
{
    if( !name ) return NULL;
    if( strlen(name) < 1 ) return NULL;
 
    if( name[0] >= '0' && name[0] <= '9' )
    {
        pid_t pid = atoi(name);
        process_t* proc = process_from_pid(pid);
        if( !proc )
        {
            return NULL;
        }
        fs_node_t* out = procfs_procdir_create(proc);
        return out;
    }

    if( !strcmp(name,"self") )
    {
        return procfs_create_self();
    }

    for( unsigned int i = 0; i < PROCFS_STANDARD_ENTRIES; ++i )
    {
        if( !strcmp(name, std_entries[i].name) )
        {
            fs_node_t* out = procfs_generic_create(std_entries[i].name, std_entries[i].func);
            return out;
        }
    }

    return NULL;
}

static fs_node_t* procfs_create( void )
{
    fs_node_t* fnode = malloc(sizeof(fs_node_t));
    memset(fnode, 0x00, sizeof(fs_node_t));
    fnode->inode = 0;
    strcpy(fnode->name, "proc");
    fnode->mask = 0555;
    fnode->uid  = 0;
    fnode->gid  = 0;
    fnode->type   = FS_DIRECTORY;
    fnode->read    = NULL;
    fnode->write   = NULL;
    fnode->open    = NULL;
    fnode->close   = NULL;
    fnode->readdir = readdir_procfs_root;
    fnode->finddir = finddir_procfs_root;
    fnode->nlink   = 1;
    fnode->ctime   = now();
    fnode->mtime   = now();
    fnode->atime   = now();
    return fnode;
}

int procfs_initialize( void )
{
    vfs_mount("/proc", procfs_create());

    return 0;
}
