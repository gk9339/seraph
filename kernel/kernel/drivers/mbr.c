#include <kernel/ata.h>
#include <kernel/fs.h>
#include <kernel/serial.h>
#include <stdio.h>
#include <kernel/mbr.h>
#include <stdlib.h>

#define SECTORSIZE 512

static mbr_t mbr;

struct mbr_partition_entry
{
    fs_node_t* device;
    partition_t partition;
};

static uint32_t read_partition( fs_node_t* node, uint32_t offset, uint32_t size, uint8_t* buffer )
{
    struct mbr_partition_entry* device = (struct mbr_partition_entry*)node->device;

    if( offset > device->partition.sector_count * SECTORSIZE )
    {
        return 0;
    }

    if( offset + size > device->partition.sector_count * SECTORSIZE )
    {
        size = device->partition.sector_count * SECTORSIZE - offset;
    }

    return read_fs(device->device, offset + device->partition.lba_first_sector * SECTORSIZE, size, buffer);
}

static uint32_t write_partition( fs_node_t* node, uint32_t offset, uint32_t size, uint8_t* buffer )
{
    struct mbr_partition_entry* device = (struct mbr_partition_entry*)node->device;

    if( offset > device->partition.sector_count * SECTORSIZE )
    {
        return 0;
    }

    if( offset + size > device->partition.sector_count * SECTORSIZE )
    {
        size = device->partition.sector_count * SECTORSIZE - offset;
    }

    return write_fs(device->device, offset + device->partition.lba_first_sector * SECTORSIZE, size, buffer);
}

static void open_partition( fs_node_t* node, unsigned int flags )
{
    return;
}

static void close_partition( fs_node_t* node )
{
    return;
}

static fs_node_t* mbr_device_create( int i, fs_node_t* node, partition_t* partition )
{
    vfs_lock(node);

    struct mbr_partition_entry* device = malloc(sizeof(struct mbr_partition_entry));
    memcpy(&device->partition, partition, sizeof(partition_t));
    device->device = node;

    fs_node_t* fnode = malloc(sizeof(fs_node_t));
    memset(fnode, 0, sizeof(fs_node_t));
    fnode->inode = 0;
    sprintf(fnode->name, "mbr%d", i);
    fnode->device = device;
    fnode->uid = 0;
    fnode->gid = 0;
    fnode->mask = 0660;
    fnode->length = device->partition.sector_count * SECTORSIZE;
    fnode->type = FS_BLOCKDEVICE;
    fnode->read = read_partition;
    fnode->write = write_partition;
    fnode->open = open_partition;
    fnode->close = close_partition;
    fnode->readdir = NULL;
    fnode->finddir = NULL;
    fnode->ioctl = NULL;

    return fnode;
}

static int read_partition_map( char* name )
{
    fs_node_t* device = kopen(name, 0);
    if( !device )
    {
        return 1;
    }

    read_fs(device, 0, SECTORSIZE, (uint8_t*)&mbr);

    if( mbr.signature[0] == 0x55 && mbr.signature[1] == 0xAA )
    {
        for( int i = 0; i < 4; i++ )
        {
            if( mbr.partitions[i].sector_count )
            {
                fs_node_t* node = mbr_device_create(i, device, &mbr.partitions[i]);

                char tmp[64];
                sprintf(tmp, "%s%d", name, i);
                vfs_mount(tmp, node);
            }else
            {
                //partition inactive
            }
        }
    }else
    {
        //no partition table
    }

    return 0;
}

void mbr_initialize( void )
{
    for( char i = 'a'; i < 'z'; i++ )
    {
        char name[64];
        sprintf(name, "/dev/hd%c", i);
        if( read_partition_map(name) )
        {
            break;
        }
    }
}
