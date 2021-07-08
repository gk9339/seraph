#include <kernel/ata.h>
#include <stdio.h>
#include <kernel/serial.h>
#include <kernel/fs.h>
#include <kernel/pci.h>
#include <list.h>
#include <stdlib.h>
#include <kernel/spinlock.h>
#include <kernel/mem.h>
#include <kernel/irq.h>
#include <kernel/process.h>
#include <arpa/inet.h>
#include <assert.h>

#define ATA_SECTOR_SIZE 512

static char ata_drive_char = 'a';
static int cdrom_number = 0;
static uint32_t ata_pci = 0;
static list_t* atapi_waiter;
static int atapi_in_progress = 0;

typedef union
{
    uint8_t command_bytes[12];
    uint16_t command_words[6];
} atapi_command_t;

typedef struct
{
    uintptr_t offset;
    uint16_t bytes;
    uint16_t last;
} prdt_t;

struct ata_device
{
    int io_base;
    int control;
    int slave;
    int is_atapi;
    ata_identity_t identity;
    prdt_t* dma_prdt;
    uintptr_t dma_prdt_phys;
    uint8_t* dma_start;
    uintptr_t dma_start_phys;
    uint32_t bar4;
    uint32_t atapi_lba;
    uint32_t atapi_sector_size;
};

static struct ata_device ata_primary_master=    {.io_base = 0x1F0, .control = 0x3F6, .slave = 0};
static struct ata_device ata_primary_slave =    {.io_base = 0x1F0, .control = 0x3F6, .slave = 1};
static struct ata_device ata_secondary_master = {.io_base = 0x170, .control = 0x376, .slave = 0};
static struct ata_device ata_secondary_slave =  {.io_base = 0x170, .control = 0x376, .slave = 1};

static spin_lock_t ata_lock = { 0 };

static void ata_device_read_sector( struct ata_device* device, uint64_t lba, uint8_t* buffer );
static void ata_device_read_sector_atapi( struct ata_device* device, uint64_t lba, uint8_t* buffer );
static void ata_device_write_sector_retry( struct ata_device* device, uint64_t lba, uint8_t* buffer );
static uint32_t read_ata( fs_node_t* node, uint32_t offset, uint32_t size, uint8_t* buffer);
static uint32_t write_ata( fs_node_t* node, uint32_t offset, uint32_t size, uint8_t* buffer );
static void open_ata( fs_node_t* node, unsigned int flags );
static void close_ata( fs_node_t* node );

static void find_ata_pci( uint32_t device, uint16_t vendorid, uint16_t deviceid, void* extra )
{
    if( (vendorid == 0x8086) && (deviceid == 0x7010 || deviceid == 0x7111) )
    {
        *((uint32_t*)extra) = device;
    }
}

static uint32_t ata_max_offset( struct ata_device* device )
{
    uint32_t sectors = device->identity.sectors_48;
    if( !sectors )
    {
        sectors = device->identity.sectors_28;
    }

    return sectors * ATA_SECTOR_SIZE;
}

static uint32_t atapi_max_offset( struct ata_device* device )
{
    uint32_t max_sector = device->atapi_lba;

    if( !max_sector )
    {
        return 0;
    }

    return (max_sector + 1) * device->atapi_sector_size;
}

static uint32_t read_ata( fs_node_t* node, uint32_t offset, uint32_t size, uint8_t* buffer )
{
    struct ata_device* device = (struct ata_device*)node->device;

    unsigned int start_block = offset / ATA_SECTOR_SIZE;
    unsigned int end_block = (offset + size - 1) / ATA_SECTOR_SIZE;

    unsigned int x_offset = 0;

    if( offset > ata_max_offset(device) )
    {
        return 0;
    }

    if( offset + size > ata_max_offset(device) )
    {
        unsigned int i = ata_max_offset(device) - offset;
        size = i;
    }

    if( offset % ATA_SECTOR_SIZE || size < ATA_SECTOR_SIZE )
    {
        unsigned int prefix_size = (ATA_SECTOR_SIZE - (offset % ATA_SECTOR_SIZE));
        if( prefix_size > size )
        {
            prefix_size = size;
        }
        char* tmp = malloc(ATA_SECTOR_SIZE);
        ata_device_read_sector(device, start_block, (uint8_t*)tmp);

        memcpy(buffer, (void*)((uintptr_t)tmp + ((uintptr_t)offset % ATA_SECTOR_SIZE)), prefix_size);

        free(tmp);

        x_offset += prefix_size;
        start_block++;
    }

    if( (offset + size) % ATA_SECTOR_SIZE && start_block <= end_block )
    {
        unsigned int postfix_size = (offset + size) % ATA_SECTOR_SIZE;

        char* tmp = malloc(ATA_SECTOR_SIZE);
        ata_device_read_sector(device, end_block, (uint8_t*)tmp);

        memcpy((void*)((uintptr_t)buffer + size - postfix_size), tmp, postfix_size);

        free(tmp);

        end_block--;
    }

    while( start_block <= end_block )
    {
        ata_device_read_sector(device, start_block, (uint8_t*)((uintptr_t)buffer + x_offset));
        x_offset += ATA_SECTOR_SIZE;
        start_block++;
    }

    return size;
}

static uint32_t read_atapi( fs_node_t* node, uint32_t offset, uint32_t size, uint8_t* buffer )
{
    struct ata_device* device = (struct ata_device*)node->device;

    unsigned int start_block = offset / device->atapi_sector_size;
    unsigned int end_block = (offset + size - 1) / device->atapi_sector_size;

    unsigned int x_offset = 0;

    if( offset > atapi_max_offset(device) )
    {
        return 0;
    }

    if( offset + size > atapi_max_offset(device) )
    {
        unsigned int i = atapi_max_offset(device) - offset;
        size = i;
    }

    if( offset % device->atapi_sector_size || size < device->atapi_sector_size )
    {
        unsigned int prefix_size = (device->atapi_sector_size - (offset % device->atapi_sector_size));
        if( prefix_size > size )
        {
            prefix_size = size;
        }

        char* tmp = malloc(device->atapi_sector_size);
        ata_device_read_sector_atapi(device, start_block, (uint8_t*)tmp);

        memcpy(buffer, (void*)((uintptr_t)tmp + ((uintptr_t)offset % device->atapi_sector_size)), prefix_size);

        free(tmp);

        x_offset += prefix_size;
        start_block++;
    }

    if( (offset + size) % device->atapi_sector_size && start_block <= end_block )
    {
        unsigned int postfix_size = (offset + size) % device->atapi_sector_size;

        char* tmp = malloc(device->atapi_sector_size);
        ata_device_read_sector_atapi(device, end_block, (uint8_t*)tmp);

        memcpy((void*)((uintptr_t)buffer + size - postfix_size), tmp, postfix_size);

        free(tmp);

        end_block--;
    }

    while( start_block <= end_block )
    {
        ata_device_read_sector_atapi(device, start_block, (uint8_t*)((uintptr_t)buffer + x_offset));
        x_offset += device->atapi_sector_size;
        start_block++;
    }

    return size;
}

static uint32_t write_ata( fs_node_t* node, uint32_t offset, uint32_t size, uint8_t* buffer )
{
    struct ata_device* device = (struct ata_device*)node->device;

    unsigned int start_block = offset / ATA_SECTOR_SIZE;
    unsigned int end_block = (offset + size - 1) / ATA_SECTOR_SIZE;

    unsigned int x_offset = 0;

    if( offset > ata_max_offset(device) )
    {
        return 0;
    }

    if( offset + size > ata_max_offset(device) )
    {
        unsigned int i = ata_max_offset(device) - offset;
        size = i;
    }

    if( offset % ATA_SECTOR_SIZE )
    {
        unsigned int prefix_size = (ATA_SECTOR_SIZE - (offset % ATA_SECTOR_SIZE));
        char* tmp = malloc(ATA_SECTOR_SIZE);

        ata_device_read_sector(device, start_block, (uint8_t*)tmp);

        memcpy((void*)((uintptr_t)tmp + ((uintptr_t)offset % ATA_SECTOR_SIZE)), buffer, prefix_size);
        ata_device_write_sector_retry(device, start_block, (uint8_t*)tmp);

        free(tmp);
        x_offset += prefix_size;
        start_block++;
    }

    if( (offset + size) % ATA_SECTOR_SIZE && start_block <= end_block )
    {
        unsigned int postfix_size = (offset + size) % ATA_SECTOR_SIZE;

        char* tmp = malloc(ATA_SECTOR_SIZE);
        ata_device_read_sector(device, end_block, (uint8_t*)tmp);

        memcpy(tmp, (void*)((uintptr_t)buffer + size - postfix_size), postfix_size);

        ata_device_write_sector_retry(device, end_block, (uint8_t*)tmp);

        free(tmp);
        end_block--;
    }

    while( start_block <= end_block )
    {
        ata_device_write_sector_retry(device, start_block, (uint8_t*)((uintptr_t)buffer + x_offset));
        x_offset += ATA_SECTOR_SIZE;
        start_block++;
    }

    return size;
}

static void open_ata( fs_node_t* node, unsigned int flags )
{
    return;
}

static void close_ata( fs_node_t* node )
{
    return;
}

static fs_node_t* ata_device_create( struct ata_device* device )
{
    fs_node_t* fnode = malloc(sizeof(fs_node_t));
    memset(fnode, 0, sizeof(fs_node_t));

    fnode->inode = 0;
    sprintf(fnode->name, "atadev%d", ata_drive_char - 'a');
    fnode->device = device;
    fnode->uid = 0;
    fnode->gid = 0;
    fnode->mask    = 0660;
    fnode->length  = ata_max_offset(device);
    fnode->type    = FS_BLOCKDEVICE;
    fnode->read    = read_ata;
    fnode->write   = write_ata;
    fnode->open    = open_ata;
    fnode->close   = close_ata;
    fnode->readdir = NULL;
    fnode->finddir = NULL;
    fnode->ioctl   = NULL;

    return fnode;
}

static fs_node_t* atapi_device_create( struct ata_device* device )
{
    fs_node_t* fnode = malloc(sizeof(fs_node_t));
    memset(fnode, 0, sizeof(fs_node_t));

    fnode->inode = 0;
    sprintf(fnode->name, "cdrom%d", cdrom_number);
    fnode->device = device;
    fnode->uid = 0;
    fnode->gid = 0;
    fnode->mask    = 0664;
    fnode->length  = atapi_max_offset(device);
    fnode->type    = FS_BLOCKDEVICE;
    fnode->read    = read_atapi;
    fnode->write   = NULL;
    fnode->open    = open_ata;
    fnode->close   = close_ata;
    fnode->readdir = NULL;
    fnode->finddir = NULL;
    fnode->ioctl   = NULL;

    return fnode;
}

static void ata_io_wait( struct ata_device* device )
{
    inportb(device->io_base + ATA_REG_ALTSTATUS);
    inportb(device->io_base + ATA_REG_ALTSTATUS);
    inportb(device->io_base + ATA_REG_ALTSTATUS);
    inportb(device->io_base + ATA_REG_ALTSTATUS);
}

static int ata_status_wait( struct ata_device* device, int timeout )
{
    int status;

    if( timeout > 0 )
    {
        int i = 0;

        while( (status = inportb(device->io_base + ATA_REG_STATUS)) & ATA_SR_BSY && (i < timeout) )
        {
            i++;
        }
    }else
    {
        while( (status = inportb(device->io_base + ATA_REG_STATUS)) & ATA_SR_BSY );
    }

    return status;
}

static int ata_wait( struct ata_device* device, int advanced )
{
    uint8_t status = 0;

    ata_io_wait(device);

    status = ata_status_wait(device, -1);

    if( advanced )
    {
        status = inportb(device->io_base + ATA_REG_STATUS);

        if( status & ATA_SR_ERR )
        {
            return 1;
        }

        if( status & ATA_SR_DF )
        {
            return 1;
        }

        if( !(status & ATA_SR_DRQ) )
        {
            return 1;
        }
    }

    return 0;
}

static void ata_soft_reset( struct ata_device* device )
{
    outportb(device->control, 0x04);
    ata_io_wait(device);
    outportb(device->control, 0x00);
}

static int ata_irq_handler( struct regs* r )
{
    inportb(ata_primary_master.io_base + ATA_REG_STATUS);

    if( atapi_in_progress )
    {
        wakeup_queue(atapi_waiter);
    }

    irq_ack(14);

    return 1;
}

static int ata_irq_handler_s( struct regs* r )
{
    inportb(ata_secondary_master.io_base + ATA_REG_STATUS);

    if( atapi_in_progress )
    {
        wakeup_queue(atapi_waiter);
    }

    irq_ack(15);

    return 1;
}

static void ata_device_init( struct ata_device* device )
{
    outportb(device->io_base + 1, 1);
    outportb(device->control, 0);

    outportb(device->io_base + ATA_REG_HDDEVSEL, 0xA0 | (device->slave << 4));
    ata_io_wait(device);

    outportb(device->io_base + ATA_REG_COMMAND, ATA_CMD_IDENTIFY);
    ata_io_wait(device);

    int status = inportb(device->io_base + ATA_REG_STATUS);
    char debug_str[512];
    debug_logf(debug_str, "ata status: %d", status);

    ata_wait(device, 0);

    uint16_t* buf = (uint16_t*)&device->identity;

    for( int i = 0; i < 256; i++ )
    {
        buf[i] = inports(device->io_base);
    }

    uint8_t* ptr = (uint8_t*)&device->identity.model;
    for( int i = 0; i < 39; i += 2 )
    {
        uint8_t tmp = ptr[i+1];
        ptr[i+1] = ptr[i];
        ptr[i] = tmp;
    }

    device->is_atapi = 0;

    debug_logf(debug_str, "Devname: %s", device->identity.model);
    debug_logf(debug_str, "Sectors: %d", device->identity.sectors_48);
    debug_logf(debug_str, "Sectors: %d", device->identity.sectors_28);

    device->dma_prdt = (void*)kvmalloc_p(sizeof(prdt_t), &device->dma_prdt_phys);
    device->dma_start = (void*)kvmalloc_p(4096, &device->dma_start_phys);

    device->dma_prdt[0].offset = device->dma_start_phys;
    device->dma_prdt[0].bytes = 512;
    device->dma_prdt[0].last = 0x8000;

    uint16_t command_reg = pci_read_field(ata_pci, PCI_COMMAND, 4);
    if( !(command_reg & (1 << 2)) )
    {
        command_reg |= (1 << 2);
        pci_write_field(ata_pci, PCI_COMMAND, 4, command_reg);
        command_reg = pci_read_field(ata_pci, PCI_COMMAND, 4);
    }

    device->bar4 = pci_read_field(ata_pci, PCI_BAR4, 4);

    if( device->bar4 & 0x00000001 )
    {
        device->bar4 = device->bar4 & 0xFFFFFFFC;
    }
}

static int atapi_device_init( struct ata_device* device )
{
    outportb(device->io_base + 1, 1);
    outportb(device->control, 0);

    outportb(device->io_base + ATA_REG_HDDEVSEL, 0xA0 | (device->slave << 4));
    ata_io_wait(device);

    outportb(device->io_base + ATA_REG_COMMAND, ATA_CMD_IDENTIFY_PACKET);
    ata_io_wait(device);

    int status = inportb(device->io_base + ATA_REG_STATUS);
    char debug_str[512];
    debug_logf(debug_str, "atapi status: %d", status);
    
    ata_wait(device, 0);

    uint16_t* buf = (uint16_t*)&device->identity;

    for( int i = 0; i < 256; i++ )
    {
        buf[i] = inports(device->io_base);
    }

    uint8_t* ptr = (uint8_t*)&device->identity.model;
    for( int i = 0; i < 39; i += 2 )
    {
        uint8_t tmp = ptr[i+1];
        ptr[i+1] = ptr[i];
        ptr[i] = tmp;
    }

    device->is_atapi = 1;
    
    debug_logf(debug_str, "Devname: %s", device->identity.model);

    atapi_command_t command;
    command.command_bytes[0] = 0x25;
    command.command_bytes[1] = 0;
    command.command_bytes[2] = 0;
    command.command_bytes[3] = 0;
    command.command_bytes[4] = 0;
    command.command_bytes[5] = 0;
    command.command_bytes[6] = 0;
    command.command_bytes[7] = 0;
    command.command_bytes[8] = 0;
    command.command_bytes[9] = 0;
    command.command_bytes[10] = 0;
    command.command_bytes[11] = 0;

    uint16_t bus = device->io_base;

    outportb(bus + ATA_REG_FEATURES, 0);
    outportb(bus + ATA_REG_LBA1, 0x08);
    outportb(bus + ATA_REG_LBA2, 0x08);
    outportb(bus + ATA_REG_COMMAND, ATA_CMD_PACKET);

    int timeout = 100;
    while( 1 )
    {
        status = inportb(device->io_base + ATA_REG_STATUS);
        
        if( status & ATA_SR_ERR )
        {
            return 1;
        }

        if( timeout-- < 100 )
        {
            return 1;
        }

        if( !(status & ATA_SR_BSY) && (status & ATA_SR_DRDY) )
        {
            break;
        }
    }

    for( int i = 0; i < 6; i++ )
    {
        outports(bus, command.command_words[i]);
    }

    while( 1 )
    {
        status = inportb(device->io_base + ATA_REG_STATUS);

        if( status & ATA_SR_ERR )
        {
            return 1;
        }

        if( timeout-- < 100 )
        {
            return 1;
        }

        if( !(status & ATA_SR_BSY) && (status & ATA_SR_DRDY) )
        {
            break;
        }

        if( status & ATA_SR_DRQ )
        {
            break;
        }
    }

    uint16_t data[4];

    for( int i = 0; i < 4; i++ )
    {
        data[i] = inports(bus);
    }

    uint32_t lba, blocks;
    memcpy(&lba, &data[0], sizeof(uint32_t));
    lba = htonl(lba);
    memcpy(&blocks, &data[2], sizeof(uint32_t));
    blocks = htonl(blocks);

    device->atapi_lba = lba;
    device->atapi_sector_size = blocks;

    if( !lba )
    {
        return 1;
    }

    return 0;
}

static int ata_device_detect( struct ata_device* device )
{
    ata_soft_reset(device);
    ata_io_wait(device);
    outportb(device->io_base + ATA_REG_HDDEVSEL, 0xA0 | (device->slave << 4));
    ata_io_wait(device);
    ata_status_wait(device, 10000);
    
    unsigned char cl = inportb(device->io_base + ATA_REG_LBA1);
    unsigned char ch = inportb(device->io_base + ATA_REG_LBA2);

    if( cl == 0xFF && ch == 0xFF )
    {
        return 0;
    }

    if( (cl == 0x00 && ch == 0x00) ||
        (cl == 0x3C && ch == 0xC3) )
    {
/*outportb(device->io_base + 1, 1);
outportb(device->control, 0);

outportb(device->io_base + ATA_REG_HDDEVSEL, 0xA0 | (device->slave << 4));
ata_io_wait(device);
*/
int status = inportb(device->io_base + ATA_REG_STATUS);
ata_io_wait(device);
if( status == 0 )
{
    return 0;
}

        char devname[64];
        sprintf((char*)&devname, "/dev/hd%c", ata_drive_char);
        fs_node_t* node = ata_device_create(device);
        vfs_mount(devname, node);
        ata_device_init(device);
        node->length = ata_max_offset(device);

        ata_drive_char++;

        return 1;
    }else if( (cl == 0x14 && ch == 0xEB) ||
              (cl == 0x69 && ch == 0x96) )
    {
        char devname[64];
        sprintf((char*)&devname, "/dev/cdrom%d", cdrom_number);
        atapi_device_init(device);
        fs_node_t* node = atapi_device_create(device);
        vfs_mount(devname, node);

        cdrom_number++;

        return 2;
    }

    // SATA
    
    return 0;
}

static void ata_device_read_sector( struct ata_device* device, uint64_t lba, uint8_t* buf )
{
    uint16_t bus = device->io_base;
    uint8_t slave = device->slave;

    if( device->is_atapi )
    {
        return;
    }

    spin_lock(ata_lock);

    ata_wait(device, 0);

    outportb(device->bar4, 0);

    outportl(device->bar4 + 0x4, device->dma_prdt_phys);
    
    outportb(device->bar4 + 0x2, inportb(device->bar4 + 0x2) | 0x4 | 0x2 );

    outportb(device->bar4, 8);

    while( 1 )
    {
        uint8_t status = inportb(device->io_base + ATA_REG_STATUS);
        
        if( !(status & ATA_SR_BSY) )
        {
            break;
        }
    }

    outportb(bus + ATA_REG_CONTROL, 0);
    outportb(bus + ATA_REG_HDDEVSEL, 0xe0 | (slave << 4));
    ata_io_wait(device);
    outportb(bus + ATA_REG_FEATURES, 0);

    outportb(bus + ATA_REG_SECCOUNT0, 0);
    outportb(bus + ATA_REG_LBA0, (lba & 0xff000000) >> 24);
    outportb(bus + ATA_REG_LBA1, (lba & 0xff00000000) >> 32);
    outportb(bus + ATA_REG_LBA2, (lba & 0xff0000000000) >> 40);

    outportb(bus + ATA_REG_SECCOUNT0, 1);
    outportb(bus + ATA_REG_LBA0, (lba & 0xff) >> 0);
    outportb(bus + ATA_REG_LBA1, (lba & 0xff00) >> 8);
    outportb(bus + ATA_REG_LBA2, (lba & 0xff0000) >> 16);

    while( 1 )
    {
        uint8_t status = inportb(device->io_base + ATA_REG_STATUS);

        if( !(status & ATA_SR_BSY) && (status & ATA_SR_DRDY) )
        {
            break;
        }
    }

    if( device->identity.sectors_48 )
    {
        outportb(bus + ATA_REG_COMMAND, ATA_CMD_READ_DMA_EXT);
    }else
    {
        outportb(bus + ATA_REG_COMMAND, ATA_CMD_READ_DMA);
    }

    ata_io_wait(device);

    outportb(device->bar4, 8 | 1);

    while( 1 )
    {
        int status = inportb(device->bar4 + 2);
        int dstatus = inportb(device->io_base + ATA_REG_STATUS);
        if( !(status & 4) )
        {
            continue;
        }
        if( !(dstatus & ATA_SR_BSY) )
        {
            break;
        }
    }

    memcpy(buf, device->dma_start, 512);

    outportb(device->bar4 + 2, inportb(device->bar4 + 2) | 4 | 2);

    spin_unlock(ata_lock);
}

static void ata_device_read_sector_atapi( struct ata_device* device, uint64_t lba, uint8_t* buf )
{
    if( !device->is_atapi )
    {
        return;
    }

    uint16_t bus = device->io_base;
    spin_lock(ata_lock);

    outportb(device->io_base + ATA_REG_HDDEVSEL, 0xA0 | device->slave << 4);
    ata_io_wait(device);

    outportb(bus + ATA_REG_FEATURES, 0);
    outportb(bus + ATA_REG_LBA1, device->atapi_sector_size & 0xFF);
    outportb(bus + ATA_REG_LBA2, device->atapi_sector_size >> 8);
    outportb(bus + ATA_REG_COMMAND, ATA_CMD_PACKET);

    while( 1 )
    {
        uint8_t status = inportb(device->io_base + ATA_REG_STATUS);
        if( (status & ATA_SR_ERR) )
        {
            spin_unlock(ata_lock);
            return;
        }
        if( !(status & ATA_SR_BSY) && (status & ATA_SR_DRQ) )
        {
            break;
        }
    }
    atapi_in_progress = 1;

    atapi_command_t command;
    command.command_bytes[0] = 0xA8;
    command.command_bytes[1] = 0;
    command.command_bytes[2] = (lba >> 0x18) & 0xFF;
    command.command_bytes[3] = (lba >> 0x10) & 0xFF;
    command.command_bytes[4] = (lba >> 0x08) & 0xFF;
    command.command_bytes[5] = (lba >> 0x00) & 0xFF;
    command.command_bytes[6] = 0;
    command.command_bytes[7] = 0;
    command.command_bytes[8] = 0;
    command.command_bytes[9] = 1;
    command.command_bytes[10] = 0;
    command.command_bytes[11] = 0;

    for( int i = 0; i < 6; i++ )
    {
        outports(bus, command.command_words[i]);
    }

    sleep_on(atapi_waiter);

    atapi_in_progress = 0;

    while( 1 )
    {
        uint8_t status = inportb(device->io_base + ATA_REG_STATUS);
        if( (status & ATA_SR_ERR) )
        {
            spin_unlock(ata_lock);
            return;
        }
        if( !(status & ATA_SR_BSY) && (status & ATA_SR_DRQ) )
        {
            break;
        }
    }

    uint16_t size_to_read = inportb(bus + ATA_REG_LBA2) << 8;
    size_to_read = size_to_read | inportb(bus + ATA_REG_LBA1);

    inportsm(bus, buf, size_to_read / 2);

    while( 1 )
    {
        uint8_t status = inportb(device->io_base + ATA_REG_STATUS);
        if( (status & ATA_SR_ERR) )
        {
            spin_unlock(ata_lock);
            return;
        }
        if( !(status & ATA_SR_BSY) && (status & ATA_SR_DRQ) )
        {
            break;
        }
    }

    spin_unlock(ata_lock);
}

static void ata_device_write_sector( struct ata_device* device, uint64_t lba, uint8_t* buf )
{
    uint16_t bus = device->io_base;
    uint8_t slave = device->slave;

    spin_lock(ata_lock);

    outportb(bus + ATA_REG_CONTROL, 2);
    
    ata_wait(device, 0);
    outportb(bus + ATA_REG_HDDEVSEL, 0xe0 | slave << 4);
    ata_wait(device, 0);

    outportb(bus + ATA_REG_FEATURES, 0);

    outportb(bus + ATA_REG_SECCOUNT0, 0);
    outportb(bus + ATA_REG_LBA0, (lba & 0xff000000) >> 24);
    outportb(bus + ATA_REG_LBA1, (lba & 0xff00000000) >> 32);
    outportb(bus + ATA_REG_LBA2, (lba & 0xff0000000000) >> 40);

    outportb(bus + ATA_REG_SECCOUNT0, 1);
    outportb(bus + ATA_REG_LBA0, (lba & 0xff) >> 0);
    outportb(bus + ATA_REG_LBA1, (lba & 0xff00) >> 8);
    outportb(bus + ATA_REG_LBA2, (lba & 0xff0000) >> 16);

    if( device->identity.sectors_48 )
    {
        outportb(bus + ATA_REG_COMMAND, ATA_CMD_WRITE_PIO_EXT);
    }else
    {
        outportb(bus + ATA_REG_COMMAND, ATA_CMD_WRITE_PIO);
    }
    ata_wait(device, 0);

    int size = ATA_SECTOR_SIZE / 2;
    outportsm(bus, buf, size);
    outportb(bus + 7, ATA_CMD_CACHE_FLUSH);
    ata_wait(device, 0);

    spin_unlock(ata_lock);
}

static int buffer_compare( uint32_t* ptr1, uint32_t* ptr2, size_t size )
{
    assert( !(size % 4) );

    size_t i = 0;

    while( i < size )
    {
        if( *ptr1 != *ptr2 )
        {
            return 1;
        }
        ptr1++;
        ptr2++;

        i += sizeof(uint32_t);
    }

    return 0;
}

static void ata_device_write_sector_retry( struct ata_device* device, uint64_t lba, uint8_t* buf )
{
    uint64_t sectors = device->identity.sectors_48;

    if( lba >= sectors )
    {
        return;
    }

    uint8_t* read_buf = malloc(ATA_SECTOR_SIZE);
    do{
        ata_device_write_sector(device, lba, buf);
        ata_device_read_sector(device, lba, read_buf);
    }while( buffer_compare((uint32_t*)buf, (uint32_t*)read_buf, ATA_SECTOR_SIZE) );

    free(read_buf);
}

void ata_initialize( void )
{
    pci_scan(&find_ata_pci, -1, &ata_pci);

    irq_install_handler(14, ata_irq_handler, "ide master");
    irq_install_handler(15, ata_irq_handler_s, "ide slave");

    atapi_waiter = list_create();

    ata_device_detect(&ata_primary_master);
    ata_device_detect(&ata_primary_slave);
    ata_device_detect(&ata_secondary_master);
    ata_device_detect(&ata_secondary_slave);
}
