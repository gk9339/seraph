#ifndef _KERNEL_LFB_H
#define _KERNEL_LFB_H

#include <stdint.h>

#define IO_LFB_WIDTH  0x5001
#define IO_LFB_HEIGHT 0x5002
#define IO_LFB_DEPTH  0x5003
#define IO_LFB_ADDR   0x5004
#define IO_LFB_STRIDE 0x5007

struct vid_size
{
    uint32_t width;
    uint32_t height;
};

extern uint8_t* lfb_address;
extern uint16_t lfb_resolution_x;
extern uint16_t lfb_resolution_y;
extern uint16_t lfb_bpp;
extern uint16_t lfb_stride;

void lfb_initialize_device( void );
void lfb_initialize( void );
void terminal_writestring( const char* data );
void terminal_kpanic_color( void );

#endif
