#include <kernel/ramdisk.h>
#include <kernel/mem.h>
#include <errno.h>
#include <kernel/task.h>
#include <kernel/process.h>
#include <string.h>
#include <stdlib.h>
#include <stdio.h>

static int last_device_number = 0;

static uint32_t read_ramdisk( fs_node_t*, uint64_t, uint32_t, uint8_t* );
static uint32_t write_ramdisk( fs_node_t*, uint64_t, uint32_t, uint8_t* );
static void open_ramdisk( fs_node_t*, unsigned int );
static void close_ramdisk( fs_node_t* );

static uint32_t read_ramdisk( fs_node_t* node, uint64_t offset, uint32_t size, uint8_t* buffer )
{
    if( offset > node->length )
    {
        return 0;
    }

    if( offset + size > node->length )
    {
        unsigned int i = node->length - offset;
        size = i;
    }

    memcpy(buffer, (void*)(node->inode + (uintptr_t)offset), size);

    return size;
}

static uint32_t write_ramdisk( fs_node_t* node, uint64_t offset, uint32_t size, uint8_t* buffer )
{
    if( offset > node->length )
    {
        return 0;
    }

    if( offset + size > node->length )
    {
        unsigned int i = node->length - offset;
        size = i;
    }

    memcpy((void*)(node->inode + (uintptr_t)offset), buffer, size);

    return size;
}

static void open_ramdisk( fs_node_t* node __attribute__((unused)), unsigned int flags __attribute__((unused)) )
{
    return;
}

static void close_ramdisk( fs_node_t* node __attribute__((unused)) )
{
    return;
}

static int ioctl_ramdisk( fs_node_t* node, int request, void* argp __attribute__((unused)) )
{
    switch( request )
    {
        case 0x4001:
            if( current_process->user != 0 )
            {
                return -EPERM;
            }else
            {
                if( node->length >= 0x1000 )
                {
                    if( node->length % 0x1000 )
                    {
                        node->length -= node->length % 0x1000;
                    }
                    for( uintptr_t i = node->inode; i < (node->inode + node->length); i += 0x1000 )
                    {
                        clear_frame(i);
                    }
                }
                node->length = 0;
                return 0;
            }
            break;
        default:
            return -EINVAL;
    }
}

static fs_node_t* ramdisk_device_create( int device_number, uintptr_t location, size_t size )
{
    fs_node_t * fnode = malloc(sizeof(fs_node_t));
    memset(fnode, 0x00, sizeof(fs_node_t));
    
    fnode->inode = location;
    sprintf(fnode->name, "ram%d", device_number);
    fnode->uid = 0;
    fnode->gid = 0;
    fnode->mask    = 0770;
    fnode->length  = size;
    fnode->flags   = FS_BLOCKDEVICE;
    fnode->read    = read_ramdisk;
    fnode->write   = write_ramdisk;
    fnode->open    = open_ramdisk;
    fnode->close   = close_ramdisk;
    fnode->ioctl   = ioctl_ramdisk;
    
    return fnode;
}

fs_node_t* ramdisk_mount( uintptr_t location, size_t size )
{
    fs_node_t* ramdisk = ramdisk_device_create(last_device_number, location, size);
    if( ramdisk )
    {
        char tmp[64];
        sprintf(tmp, "/dev/%s", ramdisk->name);
        vfs_mount(tmp, ramdisk);
        last_device_number++;

        return ramdisk;
    }

    return NULL;
}
