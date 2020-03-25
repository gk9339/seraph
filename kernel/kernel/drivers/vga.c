#include <stddef.h>
#include <string.h>

#include <kernel/types.h>
#include <kernel/multiboot.h>
#include <kernel/serial.h>
#include <kernel/vga.h>

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

    terminal_clear();
}

void terminal_clear( void )
{
    /* Clear screen */
    for (size_t y = 0; y < VGA_HEIGHT; y++) 
    {
		for (size_t x = 0; x < VGA_WIDTH; x++) 
        {
			const size_t index = y * VGA_WIDTH + x;
			terminal_buffer[index] = vga_entry(' ', terminal_color);
		}
	}
    terminal_row = 0;
    terminal_column = 0;
}

void terminal_scroll( size_t rows )
{
    for( size_t i = 0; i < rows; i++ )
    {
        memmove(terminal_buffer, (terminal_buffer + 80), sizeof(uint16_t) * 80 * 24);
        for( size_t x = 0; x < VGA_WIDTH; x++ )
        {
            const size_t index = terminal_row * VGA_WIDTH + x;
            terminal_buffer[index] = vga_entry(' ', terminal_color);
        }
    }
}

void terminal_setcolor( uint8_t color ) 
{
	terminal_color = color;
}

static void terminal_putentryat( unsigned char c, uint8_t color, size_t x, size_t y ) 
{
	const size_t index = y * VGA_WIDTH + x;
	terminal_buffer[index] = vga_entry(c, color);
}

static void update_cursor( int x, int y )
{
	uint32_t pos = y * VGA_WIDTH + x;
 
	outportb(0x3D4, 0x0F);
	outportb(0x3D5, (uint8_t)(pos & 0xFF));
	outportb(0x3D4, 0x0E);
	outportb(0x3D5, (uint8_t)((pos >> 8) & 0xFF));
}

void terminal_putchar( char c ) 
{
	unsigned char uc = (unsigned char)c;

    /* Handle special characters */
	if( uc == '\n' )
    {
        terminal_column = 0;
        if( terminal_row != VGA_HEIGHT - 1 )
        {
            terminal_row++;
        }else
        {
            terminal_scroll(1);
        }
    }else if( uc == '\b' )
    {
        if( terminal_column == 0 )
        {
            terminal_column = VGA_WIDTH - 1;
            if( terminal_row == 0 )
            {
                terminal_row = VGA_HEIGHT - 1;
            }else
            {
                terminal_row--;
            }
        }else
        {
            terminal_column--;
        }
        /* Replace whatever character was at this location with ' ' */
        terminal_putentryat(' ', terminal_color, terminal_column, terminal_row);
        
    }else if( uc == '\r' )
    {
        terminal_column = 0;
    }else if( uc > 31 ) /* Print only printable ascii characters */
    {
        terminal_putentryat(uc, terminal_color, terminal_column, terminal_row);
    
        if (++terminal_column == VGA_WIDTH) 
        {
	    	terminal_column = 0;
    		if ( terminal_row != VGA_HEIGHT - 1 )
            {
		    	terminal_row++;
            }else
            {
                terminal_scroll(1);
            }
        }
    }
    update_cursor(terminal_column, terminal_row);
}

void terminal_write( const char* data, size_t size ) 
{
	for (size_t i = 0; i < size; i++)
    {
		terminal_putchar(data[i]);
    }
}

void terminal_writestring( const char* data ) 
{
	terminal_write(data, strlen(data));
}
