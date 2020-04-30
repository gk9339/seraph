#include <kernel/multiboot.h>
#include <kernel/kernel.h>
#include <kernel/lfb.h>
#include <kernel/fs.h>
#include <kernel/syscall.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <kernel/mem.h>
#include <errno.h>
#include "terminal-font.h"

uint16_t lfb_resolution_x = 0;
uint16_t lfb_resolution_y = 0;
uint16_t lfb_bpp = 0;
uint32_t lfb_stride = 0;
uint8_t* lfb_address = (uint8_t*)0xE0000000;

static fs_node_t * lfb_device = NULL;
static int line = 0;
static uint32_t bg = 0xFF050505;
static uint32_t fg = 0xFFCCCCCC;
static int terminal_x = 0;
static int terminal_y = 0;

static fs_node_t* lfb_video_device_create( void );
static int ioctl_vid( fs_node_t* node, int request, void* argp );

void lfb_initialize_device( void )
{
    lfb_device = lfb_video_device_create();
    lfb_device->length = lfb_stride * lfb_resolution_y;
    vfs_mount("/dev/framebuffer", lfb_device);
}

void lfb_initialize( void )
{
    lfb_address = (uint8_t*)(uintptr_t)mbi->framebuffer_addr;
    lfb_resolution_x = (uint16_t)mbi->framebuffer_width;
    lfb_resolution_y = (uint16_t)mbi->framebuffer_height;
    lfb_stride = mbi->framebuffer_pitch;
    lfb_bpp = mbi->framebuffer_bpp;
    terminal_y = CHAR_HEIGHT * line;

    for( uintptr_t i = (uintptr_t)lfb_address; i <= (uintptr_t)lfb_address + lfb_resolution_x * lfb_resolution_y * 4; i += 0x1000 )
    {
        page_t* p = get_page(i, 1, kernel_directory);
        dma_frame(p, 0, 1, i);
        p->pat = 1;
        p->writethrough = 1;
        p->cachedisable = 1;
    }
}

static fs_node_t* lfb_video_device_create( void )
{
    fs_node_t* node = malloc(sizeof(fs_node_t));
    memset(node, 0, sizeof(fs_node_t));
    sprintf(node->name, "framebuffer");
    node->length = 0;
    node->type = FS_BLOCKDEVICE;
    node->mask = 0660;
    node->ioctl = ioctl_vid;
    return node;
}

static int ioctl_vid( fs_node_t* node __attribute__((unused)), int request, void* argp )
{
    switch( request )
    {
        case IO_LFB_WIDTH:
            // Get framebuffer width
            ptr_validate(argp, "lfb");
            *((size_t*)argp) = lfb_resolution_x;
            return 0;
        case IO_LFB_HEIGHT:
            // Get framebuffer height
            ptr_validate(argp, "lfb");
            *((size_t*)argp) = lfb_resolution_y;
            return 0;
        case IO_LFB_DEPTH:
            // Get framebuffer bit depth
            ptr_validate(argp, "lfb");
            *((size_t*)argp) = lfb_bpp;
            return 0;
        case IO_LFB_STRIDE:
            // Get framebuffer scanline stride
            ptr_validate(argp, "lfb");
            *((size_t*)argp) = lfb_stride;
            return 0;
        case IO_LFB_ADDR:
            // Get framebuffer address
            ptr_validate(argp, "lfb");
            *((uintptr_t*)argp) = (uintptr_t)lfb_address;
            return 0;
        default:
            return -EINVAL;
    }
}

static void set_point(int x, int y, uint32_t value)
{
    uint32_t * disp = (uint32_t *)lfb_address;
    uint32_t * cell = &disp[y * (lfb_stride / 4) + x];
    *cell = value;
}

static void write_char(int x, int y, int val, uint32_t color) {
    if (val > 128)
    {
        val = 4;
    }
    uint16_t * c = large_font[val];
    for (uint8_t i = 0; i < CHAR_HEIGHT; ++i)
    {
        for (uint8_t j = 0; j < CHAR_WIDTH; ++j)
        {
            if (c[i] & (1 << (15-j)))
            {
                set_point(x+j,y+i,color);
            }else
            {
                set_point(x+j,y+i,bg);
            }
        }
    }
}

void terminal_kpanic_color( void )
{
    fg = 0xcc0000;
}

void terminal_writestring( const char* data )
{
    while( *data )
    {
        if( *data == '\n')
        {
            line++;
            terminal_y = CHAR_HEIGHT * line;
            terminal_x = 0;
        }else if( *data == '\b' )
        {
            if( terminal_x != 0 )
            {
                terminal_x -= CHAR_WIDTH;
            }
            write_char(terminal_x, terminal_y, ' ', fg);
        }else if( *data == '\r' )
        {
            terminal_x = 0;
        }else if( *data == '\f' )
        {
            terminal_x = 0;
            terminal_y = 0;
            line = 0;
        }else
        {
            write_char(terminal_x, terminal_y, *data, fg);
            terminal_x += CHAR_WIDTH;
        }
        data++;
    }
}
