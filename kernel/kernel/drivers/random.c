#include <kernel/fs.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <kernel/cmos.h>
#include <kernel/random.h>

uint32_t __attribute__((pure)) xorshift( void )
{
    static uint32_t x = 123456789;
    static uint32_t y = 362436069;
    static uint32_t z = 521288629;
    static uint32_t w = 88675123;

    if( x == 123456789 )
    {
        x = now();
    }

    uint32_t t;

    t = x ^ (x << 11);
    x = y; y = z; z = w;
    return w = w ^ (w >> 19) ^ t ^ (t >> 8);
}

static uint32_t read_random( fs_node_t* node __attribute__((unused)), uint32_t offset __attribute__((unused)), uint32_t size, uint8_t* buffer )
{
    uint32_t s = 0;
    while( s < size )
    {
        buffer[s] = (uint8_t)xorshift() % 0xFF;
        s++;
    }
    return size;
}

static uint32_t write_random( fs_node_t* node __attribute__((unused)), uint32_t offset __attribute__((unused)), uint32_t size, uint8_t* buffer __attribute__((unused)) )
{
    return size;
}

static void open_random( fs_node_t* node __attribute__((unused)), unsigned int flags __attribute__((unused)) )
{
    return;
}

static void close_random( fs_node_t* node __attribute__((unused)) )
{
    return;
}

static int readlink_urandom( fs_node_t* node __attribute__((unused)), char* buf, size_t size __attribute__((unused)) )
{
    strcpy(buf, "/dev/random");
    return strlen("/dev/random");
}

static fs_node_t* random_device_create( void )
{
    fs_node_t * fnode = malloc(sizeof(fs_node_t));
    memset(fnode, 0x00, sizeof(fs_node_t));
    fnode->inode = 0;
    strcpy(fnode->name, "random");
    fnode->uid = 0;
    fnode->gid = 0;
    fnode->mask = 0444;
    fnode->length  = 1024;
    fnode->type   = FS_CHARDEVICE;
    fnode->read    = read_random;
    fnode->write   = write_random;
    fnode->open    = open_random;
    fnode->close   = close_random;
    fnode->readdir = NULL;
    fnode->finddir = NULL;
    fnode->ioctl   = NULL;
    return fnode;
}

static fs_node_t* urandom_device_create( void )
{
    fs_node_t* fnode = malloc(sizeof(fs_node_t));
    memset(fnode, 0x00, sizeof(fs_node_t));
    fnode->inode = 0;
    strcpy(fnode->name, "self");
    fnode->mask = 0777;
    fnode->uid  = 0;
    fnode->gid  = 0;
    fnode->type   = FS_CHARDEVICE | FS_SYMLINK;
    fnode->readlink = readlink_urandom;
    fnode->nlink   = 1;
    fnode->ctime   = now();
    fnode->mtime   = now();
    fnode->atime   = now();
    return fnode;
}

int random_initialize( void )
{
    vfs_mount("/dev/random", random_device_create());
    vfs_mount("/dev/urandom", urandom_device_create());
    return 0;
}
