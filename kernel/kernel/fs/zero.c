#include <kernel/fs.h>
#include <string.h>
#include <stdlib.h>
#include <sys/types.h>

static uint32_t read_zero( fs_node_t* node __attribute__((unused)), uint32_t offset __attribute__((unused)), uint32_t size, uint8_t* buffer __attribute__((unused)) )
{
    memset(buffer, 0x00, size);

    return 1;
}

static uint32_t write_zero( fs_node_t* node __attribute__((unused)), uint32_t offset __attribute__((unused)), uint32_t size __attribute__((unused)), uint8_t* buffer __attribute__((unused)) ) 
{
    return 0;
}

static void open_zero( fs_node_t* node __attribute__((unused)), unsigned int flags __attribute__((unused)) )
{
    return;
}

static void close_zero( fs_node_t* node __attribute__((unused)) )
{
    return;
}

static fs_node_t* zero_device_create( void ) 
{
    fs_node_t* fnode = malloc(sizeof(fs_node_t));
    memset(fnode, 0x00, sizeof(fs_node_t));
    fnode->inode = 0;
    strcpy(fnode->name, "zero");
    fnode->uid = 0;
    fnode->gid = 0;
    fnode->mask = 0666;
    fnode->type    = FS_CHARDEVICE;
    fnode->read    = read_zero;
    fnode->write   = write_zero;
    fnode->open    = open_zero;
    fnode->close   = close_zero;
    fnode->readdir = NULL;
    fnode->finddir = NULL;
    fnode->ioctl   = NULL;
    return fnode;
}

void zero_initialize( void ) 
{
    vfs_mount("/dev/zero", zero_device_create());
}
