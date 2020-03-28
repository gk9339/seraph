#include <unistd.h>
#include <kernel/vga.h>
#include <string.h>
#include <stdio.h>
#include <fcntl.h>
#include <pty.h>
#include <sys/signals.h>
#include <signal.h>
#include <sys/fswait.h>
#include <terminal/keyboard.h>

static const size_t VGA_WIDTH = 80;
static const size_t VGA_HEIGHT = 25;
static uint16_t* const VGA_MEMORY = (uint16_t*)0xB8000;

static size_t terminal_row;
static size_t terminal_column;
static uint8_t terminal_color;
static uint16_t* terminal_buffer;

static int fd_master, fd_slave;
static FILE* terminal;
static int input_stopped = 0;
volatile int exit_terminal = 0;

static void sig_suspend_input( int sig );
static void sig_child_exit( int sig );

void handle_input_char(char c);
void handle_input_string(char * c);

int main( void )
{
    terminal_row = 0;
    terminal_column = 0;
    terminal_color = vga_entry_color(VGA_COLOR_LIGHT_GREY, VGA_COLOR_BLACK);
    terminal_buffer = VGA_MEMORY;

    openpty(&fd_master, &fd_slave, NULL, NULL, NULL);
    terminal = fdopen(fd_slave, "w");

    struct winsize pty_winsize;
    pty_winsize.ws_row = VGA_HEIGHT;
    pty_winsize.ws_col = VGA_WIDTH;
    pty_winsize.ws_xpixel = 0;
    pty_winsize.ws_ypixel = 0;
    ioctl(fd_master, TIOCSWINSZ, &pty_winsize);

    terminal_clear();

    fflush(stdin);

    signal(SIGUSR2, sig_suspend_input);
    signal(SIGCHLD, sig_child_exit);

    uint32_t f = fork();

    if( f == 0 )
    {
        setsid();
        dup2(fd_slave, 0);
        dup2(fd_slave, 1);
        dup2(fd_slave, 2);

        char* arg[] = { NULL };
        execvp("/bin/sh", arg);
    }else
    {
        int kfd = open("/dev/kbd", O_RDONLY);
        int ret;
        char c;

        int fds[] = {fd_master, kfd};
        char buf[1024];
        key_event_state_t kbd_state = {0};
        key_event_t event;

        while( !exit_terminal )
        {
            int index = fswait2(2, fds, 200);

            if( input_stopped ) continue;

            if( index == 0 )
            {
                ret = read(fd_master, buf, 1024);
                terminal_write(buf, ret);
            }else if ( index == 1 )
            {
                ret = read(kfd, &c, 1);

                if( ret > 0 )
                {
                    ret = kbd_scancode(&kbd_state, c, &event);
                    key_event(ret, &event);
                }
            }
        }
    }
    
    return 0;
}

static inline void outportb( unsigned short _port, unsigned char _data )
{
    asm volatile("outb %1, %0" :: "dN"(_port), "a"(_data));
}

static void update_cursor( int x, int y )
{
     uint16_t pos = y * VGA_WIDTH + x;
 
     outportb(0x3D4, 0x0F);
     outportb(0x3D5, (uint8_t)(pos & 0xFF));
     outportb(0x3D4, 0x0E);
     outportb(0x3D5, (uint8_t)((pos >> 8) & 0xFF));
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
    update_cursor(terminal_row, terminal_column);
    
    signal(SIGUSR1, (void(*)(int))terminal_clear);
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
}

void terminal_write( const char* data, size_t size ) 
{
	for (size_t i = 0; i < size; i++)
    {
		terminal_putchar(data[i]);    
    }
    update_cursor(terminal_column, terminal_row);
}

void terminal_writestring( const char* data ) 
{
	terminal_write(data, strlen(data));
}

static void sig_suspend_input( int sig __attribute__((unused)) )
{
    char* exit_message = "[Input stopped]\n";
    write(fd_slave, exit_message, sizeof(exit_message));

    input_stopped = 1;

    signal(SIGUSR2, sig_suspend_input);
}

static void sig_child_exit( int sig __attribute__ ((unused)) )
{
    terminal_writestring("\n[Process completed]");

	outportb(0x3D4, 0x0A);
	outportb(0x3D5, 0x20);

    exit_terminal = 1;
}

void handle_input_char( char c )
{
	write(fd_master, &c, 1);
}

void handle_input_string( char* c )
{
	write(fd_master, c, strlen(c));
} 
