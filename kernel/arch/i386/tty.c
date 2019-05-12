#include <stddef.h>
#include <string.h>

#include <kernel/tty.h>
#include <kernel/types.h>
#include <kernel/multiboot.h>
#include <kernel/serial.h>

#include "vga.h"

static const size_t VGA_WIDTH = 80;
static const size_t VGA_HEIGHT = 25;
static uint16_t* const VGA_MEMORY = (uint16_t*)0xB8000;

static size_t terminal_row;
static size_t terminal_column;
static uint8_t terminal_color;
static uint16_t* terminal_buffer;

void terminal_initialize( void ) 
{
    /* Initial row/col/buffer/memory buffer setup */
	terminal_row = 0;
	terminal_column = 0;
	terminal_color = vga_entry_color(VGA_COLOR_LIGHT_GREY, VGA_COLOR_BLACK);
    terminal_buffer = VGA_MEMORY;

    /* Clear screen */
    for (size_t y = 0; y < VGA_HEIGHT; y++) 
    {
		for (size_t x = 0; x < VGA_WIDTH; x++) 
        {
			const size_t index = y * VGA_WIDTH + x;
			terminal_buffer[index] = vga_entry(' ', terminal_color);
		}
	}

    /* Disable cursor */
    outportb(0x3D4, 0x0A);
    outportb(0x3D5, 0x20);
}

void terminal_setcolor( uint8_t color ) 
{
	terminal_color = color;
}

void terminal_putentryat( unsigned char c, uint8_t color, size_t x, size_t y ) 
{
	const size_t index = y * VGA_WIDTH + x;
	terminal_buffer[index] = vga_entry(c, color);
}

void terminal_putchar( char c ) 
{
	unsigned char uc = c;

    /* Handle special characters */
	if( uc == '\n' )
    {
        terminal_column = 0;
        if( ++terminal_row == VGA_HEIGHT )
            terminal_row = 0;
        /* Clear new rows */
        for( int i = 0; i < VGA_WIDTH; i++ )
        {
            terminal_putentryat(' ', terminal_color, i, terminal_row);
        }
    }else if( uc == '\b' )
    {
        if( terminal_column == 0 )
        {
            terminal_column = VGA_WIDTH - 1;
            if( terminal_row == 0 )
                terminal_row = VGA_HEIGHT - 1;
            else
                terminal_row--;
        }else
            terminal_column--;
        /* Replace whatever character was at this location with ' ' */
        terminal_putentryat(' ', terminal_color, terminal_column, terminal_row);
    }else if( uc > 31 ) /* Print only printable ascii characters */
    {
        terminal_putentryat(uc, terminal_color, terminal_column, terminal_row);
    
        if (++terminal_column == VGA_WIDTH) 
        {
	    	terminal_column = 0;
    		if (++terminal_row == VGA_HEIGHT)
		    	terminal_row = 0;
        }
    }
}

void terminal_write( const char* data, size_t size ) 
{
	for (size_t i = 0; i < size; i++)
		terminal_putchar(data[i]);
}

void terminal_writestring( const char* data ) 
{
	terminal_write(data, strlen(data));
}
