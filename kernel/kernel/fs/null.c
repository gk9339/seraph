#include <kernel/fs.h>
#include <string.h>
#include <stdlib.h>
#include <sys/types.h>

static uint32_t read_null( fs_node_t* node __attribute__((unused)), uint32_t offset __attribute__((unused)), uint32_t size __attribute__((unused)), uint8_t* buffer __attribute__((unused)) )
{
    return 0;
}

static uint32_t write_null( fs_node_t* node __attribute__((unused)), uint32_t offset __attribute__((unused)), uint32_t size __attribute__((unused)), uint8_t* buffer __attribute__((unused)) )
{
    return 0;
}

static void open_null(fs_node_t* node __attribute__((unused)), unsigned int flags __attribute__((unused)))
{
    return;
}

static void close_null(fs_node_t* node __attribute__((unused)))
{
    return;
}

static fs_node_t* null_device_create( void ) 
{
    fs_node_t* fnode = malloc(sizeof(fs_node_t));
    memset(fnode, 0x00, sizeof(fs_node_t));
    fnode->inode = 0;
    strcpy(fnode->name, "null");
    fnode->uid = 0;
    fnode->gid = 0;
    fnode->mask = 0666;
    fnode->type   = FS_CHARDEVICE;
    fnode->read    = read_null;
    fnode->write   = write_null;
    fnode->open    = open_null;
    fnode->close   = close_null;
    fnode->readdir = NULL;
    fnode->finddir = NULL;
    fnode->ioctl   = NULL;
    return fnode;
}

void null_initialize( void ) 
{
    vfs_mount("/dev/null", null_device_create());
}
