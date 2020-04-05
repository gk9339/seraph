#include <unistd.h>
#include <string.h>
#include <stdio.h>
#include <fcntl.h>
#include <pty.h>
#include <time.h>
#include <sys/signals.h>
#include <signal.h>
#include <list.h>
#include <stdlib.h>
#include <sys/fswait.h>
#include <stdint.h>
#include <math.h>
#include <wchar.h>
#include <libkbd/libkbd.h>
#include <libansiterm/libansiterm.h>
#include "terminal.h"
#include "decodeutf8.h"
#include "ununicode.h"

#define SWAP(T,a,b) { T _a = a; a = b; b = _a; }

const size_t VGA_WIDTH = 80;
const size_t VGA_HEIGHT = 25;
uint16_t* const VGA_MEMORY = (uint16_t*)0xB8000;
uint16_t* mirrorcopy = NULL;

uint16_t terminal_width = VGA_WIDTH;
uint16_t terminal_height = VGA_HEIGHT;
uint16_t csr_x = 0;
uint16_t csr_y = 0;
uint16_t _orig_csr_x = 0;
uint16_t _orig_csr_y = 0;
uint32_t fg = VGA_COLOR_LIGHT_GREY;
uint32_t bg = VGA_COLOR_BLACK;
uint32_t _orig_fg = VGA_COLOR_LIGHT_GREY;
uint32_t _orig_bg = VGA_COLOR_BLACK;
uint8_t cursor_enabled = 1;
uint8_t cursor_flipped = 0;
uint64_t mouse_ticks = 0;

term_cell_t* terminal_buffer = NULL;
term_cell_t* terminal_buffer_a = NULL;
term_cell_t* terminal_buffer_b = NULL;
int active_buffer = 0;

term_state_t* ansi_state = NULL;

int selection = 0;
int selection_start_x = 0;
int selection_start_y = 0;
int selection_end_x = 0;
int selection_end_y = 0;
char * selection_text = NULL;

int fd_master, fd_slave;
FILE* terminal;
int input_stopped = 0;
volatile int exit_terminal = 0;

struct input_data
{
    size_t len;
    char data[];
};

// Signal handlers
void sig_suspend_input( int sig );
void sig_child_exit( int sig );

// Callback functions
void terminal_write( char );
void terminal_set_colors( uint32_t, uint32_t );
void set_csr( int, int );
uint16_t get_csr_x( void );
uint16_t get_csr_y( void );
void terminal_set_cell( int, int, uint32_t );
void terminal_clear( int );
void terminal_scroll( int );
void draw_cursor( void );
void handle_input_string( char* );
void set_title( char* );
//void terminal_terminal_set_cell_contents( int, int, char* );
//uint16_t terminal_get_cell_width( void );
//uint16_t terminal_get_cell_height( void );
void enable_csr( int );
void terminal_switch_buffer( int );
void insert_delete_lines( int );

// Callback functions for unimplemented features
void null( int x __attribute__((unused)), int y __attribute__((unused)), char* data __attribute__((unused)) ) {}
uint16_t null_int( void ) { return 0; }

void handle_input( char );
void write_input_buffer( char*, size_t );
void vga_write( unsigned char c, int x, int y, int attr );
void terminal_write_char( uint32_t val, uint16_t x, uint16_t y, uint32_t fg, uint32_t bg, uint8_t flags );
void cell_set( uint16_t x, uint16_t y, uint32_t codepoint, uint8_t flags );
void cell_redraw( uint16_t x, uint16_t y );
void cell_redraw_inverted( uint16_t x, uint16_t y );
void terminal_redraw_all( void );
void terminal_shift_region( int top, int height, int lines );
uint64_t get_ticks( void );
void render_cursor( void );
void flip_cursor( void );
int best_match( uint32_t a );
int is_wide( uint32_t codepoint );
static inline void outportb( unsigned short _port, unsigned char _data ); // Utility function, output byte to serial port
static inline unsigned char inportb( unsigned short _port );

term_callbacks_t terminal_callbacks =
{
    terminal_write,
    terminal_set_colors,
    set_csr,
    get_csr_x,
    get_csr_y,
    terminal_set_cell,
    terminal_clear,
    terminal_scroll,
    draw_cursor,
    handle_input_string,
    set_title,
    null,
    null_int,
    null_int,
    enable_csr,
    terminal_switch_buffer,
    insert_delete_lines,
};

int main( void )
{
    // Set TERM environment variable
    putenv("TERM=seraph-term");

    // Disable builtin VGA cursor
    outportb(0x3D4, 0x0A);
    outportb(0x3D5, 0x20);

    // Open pty, setup pty slave FILE pointer
    openpty(&fd_master, &fd_slave, NULL, NULL, NULL);
    terminal = fdopen(fd_slave, "w");

    struct winsize pty_winsize;
    pty_winsize.ws_row = VGA_HEIGHT;
    pty_winsize.ws_col = VGA_WIDTH;
    pty_winsize.ws_xpixel = 0;
    pty_winsize.ws_ypixel = 0;
    ioctl(fd_master, TIOCSWINSZ, &pty_winsize);

    // Initialize buffers and ANSI terminal state
    terminal_buffer_a = malloc(sizeof(term_cell_t) * terminal_width * terminal_height);
    memset(terminal_buffer_a, 0, sizeof(term_cell_t) * terminal_width * terminal_height);
    terminal_buffer_b = malloc(sizeof(term_cell_t) * terminal_width * terminal_height);
    memset(terminal_buffer_a, 0, sizeof(term_cell_t) * terminal_width * terminal_height);
    terminal_buffer = terminal_buffer_a;

    mirrorcopy = malloc(sizeof(unsigned short) * terminal_width * terminal_height);
    memset(mirrorcopy, 0, sizeof(unsigned short) * terminal_width * terminal_height);

    ansi_state = ansi_init(ansi_state, terminal_width, terminal_height, &terminal_callbacks);

    // Setup signal handlers
    signal(SIGUSR2, sig_suspend_input);
    signal(SIGCHLD, sig_child_exit);

    uint32_t f = fork();
    if( f == 0 )
    {
        // Child process
        setsid(); // Start new session
        // Setup pty slave as standard streams for shell
        dup2(fd_slave, 0);
        dup2(fd_slave, 1);
        dup2(fd_slave, 2);
     
        char* arg[] = { NULL };
        execvp("/bin/sh", arg);
    }else
    {
        // Parent process
        int kfd = open("/dev/kbd", O_RDONLY);
        int ret;
        char c;
     
        terminal_clear(2);

        int fds[] = {fd_master, kfd};
        char buf[1024];
        key_event_state_t kbd_state = {0};
        key_event_t event;
     
        while( !exit_terminal )
        {
            // Wait on keyboard/pty master, or 200ms
            int index = fswait2(2, fds, 200);
     
            if( input_stopped ) continue;
    
            flip_cursor();
            if( index == 0 )
            {
                // Read data out of pty master buffer and write it
                ret = read(fd_master, buf, 1024);
                for( int i = 0; i < ret; i++ )
                {
                    ansi_put(ansi_state, buf[i]);
                }
            }else if( index == 1 )
            {
                // Read data from keyboard pipe, handle key event
                ret = read(kfd, &c, 1);
     
                if( ret > 0 )
                {
                    ret = kbd_scancode(&kbd_state, c, &event);
                    key_event(ret, &event, handle_input_string, handle_input);
                }
            }
        }
    }
    return 0;
}

void sig_suspend_input( int sig __attribute__((unused)) )
{
    if( !input_stopped )
    {
        char* message = "\n[Input stopped]\n";
        write(fd_slave, message, sizeof(message));

        input_stopped = 1;
    }else
    {
        char* message = "\n[Input resumed]\n";
        write(fd_slave, message, sizeof(message));

        input_stopped = 0;
    }
    
    signal(SIGUSR2, sig_suspend_input);
}

// Child process (shell) exited
void sig_child_exit( int sig __attribute__((unused)) )
{
    char* message = "\n[Process completed]";
    for( size_t i = 0; i < strlen(message); i++ )
    {
        terminal_write(message[i]);
    }

    cell_redraw(csr_x, csr_y);
    exit_terminal = 1;
}

// Write character to screen
void terminal_write( char c )
{
    static uint32_t codepoint = 0;
    static uint32_t unicode_state = 0;

    cell_redraw(csr_x, csr_y);
    if( !decode(&unicode_state, &codepoint, (uint8_t)c) )
    {
        if( c == '\r' )
        {
            csr_x = 0;
            draw_cursor();
            return;
        }
        if( csr_x == terminal_width )
        {
            csr_x = 0;
            csr_y++;
            if( c == '\n' )
            {
                return;
            }
        }
        if( csr_y == terminal_height )
        {
            terminal_scroll(1);
            csr_y = terminal_height - 1;
        }
        if( c == '\n' )
        {
            csr_y++;
            if( csr_y == terminal_height )
            {
                terminal_scroll(1);
                csr_y = terminal_height - 1;
            }
            draw_cursor();
        }else if( c == '\007' )
        {
            uint32_t div = 1193180 / 2000;
            outportb(0x43, 0xb6);
            outportb(0x42, (uint8_t)div);
            outportb(0x42, (uint8_t)(div>>8));
            uint8_t tmp = inportb(0x61);
            if( tmp != (tmp | 3) )
            {
                outportb(0x61, tmp | 3);
            }

            usleep(15000);

            tmp = inportb(0x61) & 0xFC;
            outportb(0x61, tmp);
        }else if( c == '\b' )
        {
            if( csr_x > 0 )
            {
                csr_x--;
            }
            cell_redraw(csr_x, csr_y);
            draw_cursor();
        }else if( c == '\t' )
        {
            csr_x += ( 8 - csr_x % 8 );
            draw_cursor();
        }else
        {
            int wide = is_wide(codepoint);
            uint8_t flags = ansi_state->flags;
            if( wide && csr_x == terminal_width - 1 )
            {
                csr_x = 0;
                csr_y++;
            }
            if( wide )
            {
                flags = flags | ANSI_WIDE;
            }

            cell_set(csr_x, csr_y, codepoint, flags);
            cell_redraw(csr_x, csr_y);
            csr_x++;
            
            if( wide && csr_x != terminal_width )
            {
                cell_set(csr_x, csr_y, 0xFFFF, ansi_state->flags);
                cell_redraw(csr_x, csr_y);
                cell_redraw(csr_x-1, csr_y);
                csr_x++;
            }
        }
    }else if( unicode_state == UTF8_REJECT )
    {
        unicode_state = 0;
    }
    draw_cursor();
}

// Set terminal colours to new values
void terminal_set_colors( uint32_t new_fg, uint32_t new_bg )
{
    fg = new_fg;
    bg = new_bg;
}

// Set cursor x, y
void set_csr( int x, int y )
{
    cell_redraw(csr_x, csr_y);
    csr_x = x;
    csr_y = y;
    draw_cursor();
}

// Get cursor x
uint16_t get_csr_x( void )
{
    return csr_x;
}

// Get cursor y
uint16_t get_csr_y( void )
{
    return csr_y;
}

// Set cell at coordinates to character
void terminal_set_cell( int x, int y, uint32_t c )
{
    cell_set(x, y, c, ansi_state->flags);
    cell_redraw(x, y);
}

// 2 = full clear buffer and redraw, 1 = clear everything up to the cursor, 0 = clear all positions
void terminal_clear( int i )
{
    if( i == 2 )
    {
        csr_x = 0;
        csr_y = 0;
        memset((void*)terminal_buffer, 0, terminal_width * terminal_height * sizeof(term_cell_t));
        terminal_redraw_all();
    }else if( i == 1 )
    {
        for( int y = 0; y < csr_y; y++ )
        {
            for( int x = 0; x < terminal_width; x++ )
            {
                terminal_set_cell(x, y, ' ');
            }
        }
        for( int x = 0; x < csr_x; x++ )
        {
            terminal_set_cell(x, csr_y, ' ');
        }
    }else if( i == 0 )
    {
        for( int x = 0; x < terminal_width; x++ )
        {
            terminal_set_cell(x, csr_y, ' ');
        }
        for( int y = 0; y < terminal_height; y++ )
        {
            for( int x = 0; x < terminal_width; x++ )
            {
                terminal_set_cell(x, y, ' ');
            }
        }
    }
}

// Scrolling is shifting lines
void terminal_scroll( int lines )
{
    terminal_shift_region(0, terminal_height, lines);
}

void handle_input( char c )
{
    write(fd_master, &c, 1);
}

// Write string into pty master buffer
void handle_input_string( char* c )
{
    write(fd_master, c, strlen(c));
}

void set_title( char* title __attribute__((unused)) )
{
    return;
}

void enable_csr( int enable )
{
    cursor_enabled = enable;
    if( enable )
    {
        draw_cursor();
    }
}

void terminal_switch_buffer( int buffer )
{
    if( buffer != 0 && buffer != 1 )
    {
        return;
    }
    if( buffer != active_buffer )
    {
        active_buffer = buffer;
        terminal_buffer = active_buffer == 0 ? terminal_buffer_a : terminal_buffer_b;

        SWAP(int, csr_x, _orig_csr_x);
        SWAP(int, csr_y, _orig_csr_y);
        SWAP(uint32_t, fg, _orig_fg);
        SWAP(uint32_t, bg, _orig_bg);

        terminal_redraw_all();
    }
}

void insert_delete_lines( int lines )
{
    if( lines == 0 )
    {
        return;
    }

    terminal_shift_region(csr_y, terminal_height - csr_y, - lines);
}

void terminal_write_char( uint32_t val, uint16_t x, uint16_t y, uint32_t vga_fg, uint32_t vga_bg, uint8_t flags __attribute__((unused)) )
{
    if( val == L'▏' )
    {
        val = 179;
    }else if( val > 128 )
    {
        val = ununicode(val);
    }if( vga_fg > 256 )
    {
    	vga_fg = best_match(vga_fg);
    }
    if( vga_bg > 256 )
    {
    	vga_bg = best_match(vga_bg);
    }
    if( vga_fg > 16 )
    {
    	vga_fg = eightbit_to_vga[vga_fg];
    }
    if( vga_bg > 16 )
    {
    	vga_bg = eightbit_to_vga[vga_bg];
    }
    if( vga_fg == 16 )
    {
        vga_fg = 0;
    }if( vga_bg == 16 )
    {
        vga_bg = 0;
    }
    vga_write(val, x, y, (vga_to_ansi[vga_fg] & 0xF) | (vga_to_ansi[vga_bg] << 4));
}

void vga_write( unsigned char c, int x, int y, int attr )
{
    unsigned int where = y * terminal_width + x;
    unsigned int att = (c | (attr << 8));

    if( mirrorcopy[where] != att )
    {
        mirrorcopy[where] = att;
        VGA_MEMORY[where] = att;
    }
}

void cell_set( uint16_t x, uint16_t y, uint32_t codepoint, uint8_t flags __attribute__((unused)) )
{
    if( x >= terminal_width || y >= terminal_height )
    {
        return;
    }
    term_cell_t* cell = (term_cell_t*)((uintptr_t)terminal_buffer + (y * terminal_width + x) * sizeof(term_cell_t));
    cell->c = codepoint;
    cell->fg = fg;
    cell->bg = bg;
    cell->flags = ansi_state->flags;
}

void cell_redraw( uint16_t x, uint16_t y )
{
    if( x >= terminal_width || y >= terminal_height )
    {
        return;
    }

    term_cell_t* cell = (term_cell_t*)((uintptr_t)terminal_buffer + (y * terminal_width + x) * sizeof(term_cell_t));

    if( ((uint32_t*)cell)[0] == 0 )
    {
        terminal_write_char(' ', x, y, TERM_DEFAULT_FG, TERM_DEFAULT_BG, TERM_DEFAULT_FLAGS);
    }else
    {
        terminal_write_char(cell->c, x, y, cell->fg, cell->bg, cell->flags);
    }
}

void cell_redraw_inverted( uint16_t x, uint16_t y )
{
    if( x >= terminal_width || y >= terminal_height )
    {
        return;
    }

    term_cell_t* cell = (term_cell_t*)((uintptr_t)terminal_buffer + (y * terminal_width + x) * sizeof(term_cell_t));

    if( ((uint32_t*)cell)[0] == 0 )
    {
        terminal_write_char(' ', x, y, TERM_DEFAULT_BG, TERM_DEFAULT_FG, TERM_DEFAULT_FLAGS | ANSI_SPECBG);
    }else
    {
        terminal_write_char(cell->c, x, y, cell->bg, cell->fg, cell->flags | ANSI_SPECBG);
    }
}

void terminal_redraw_all( void )
{
    for( uint16_t y = 0; y < terminal_height; y++ )
    {
        for( uint16_t x = 0; x < terminal_width; x++ )
        {
            cell_redraw(x, y);
        }
    }
}

void terminal_shift_region( int top, int height, int lines )
{
    if( lines == 0 )
    {
        return;
    }

    int destination, source, count, new_top, new_bottom;
    if( lines > height )
    {
        count = 0;
        new_top = top;
        new_bottom = top + height;
    }else if( lines > 0 )
    {
        destination = terminal_width * top;
        source = terminal_width * (top + lines);
        count = height - lines;
        new_top = top + height - lines;
        new_bottom = top + height;
    }else
    {
        destination = terminal_width * (top - lines);
        source = terminal_width * top;
        count = height + lines;
        new_top = top;
        new_bottom = top - lines;
    }

    // Move from top + lines to top
    if( count )
    {
        memmove(terminal_buffer + destination, terminal_buffer + source, count * terminal_width * sizeof(term_cell_t));
    }
    
    // Make the erased line have default fg colour
    uint32_t fg_backup = fg;
    fg = TERM_DEFAULT_FG;
    for( int i = new_top; i < new_bottom; i++ )
    {
        for( uint16_t x = 0; x < terminal_width; x++ )
        {
            cell_set(x, i, ' ', ansi_state->flags);
        }
    }
    fg = fg_backup;

    terminal_redraw_all();
}

uint64_t get_ticks( void )
{
    struct timeval now;
    gettimeofday(&now, NULL);

    return ((uint64_t)now.tv_sec * 1000000LL) + (uint64_t)now.tv_usec;
}

void draw_cursor()
{
    if( !cursor_enabled )
    {
        return;
    }
    mouse_ticks = get_ticks();
    cursor_flipped = 1;
    render_cursor();
}

void render_cursor()
{
    if( !cursor_enabled )
    {
        return;
    }
    cell_redraw_inverted(csr_x, csr_y);
}

void flip_cursor( void )
{
    uint64_t ticks = get_ticks();
    if( ticks > mouse_ticks + 300000LL )
    {
        mouse_ticks = ticks;
        if( cursor_flipped )
        {
            cell_redraw(csr_x, csr_y);
        }else
        {
            render_cursor();
        }
        cursor_flipped = 1 - cursor_flipped;
    }
}

static int color_distance(uint32_t a, uint32_t b) {
    int a_r = (a & 0xFF0000) >> 16;
    int a_g = (a & 0xFF00) >> 8;
    int a_b = (a & 0xFF);

    int b_r = (b & 0xFF0000) >> 16;
    int b_g = (b & 0xFF00) >> 8;
    int b_b = (b & 0xFF);

    int distance = 0;
    distance += abs(a_r - b_r) * 3;
    distance += abs(a_g - b_g) * 6;
    distance += abs(a_b - b_b) * 10;

    return distance;
}

static uint32_t vga_base_colors[] = {
    0x000000,
    0xAA0000,
    0x00AA00,
    0xAA5500,
    0x0000AA,
    0xAA00AA,
    0x00AAAA,
    0xAAAAAA,
    0x555555,
    0xFF5555,
    0x55AA55,
    0xFFFF55,
    0x5555FF,
    0xFF55FF,
    0x55FFFF,
    0xFFFFFF,
};

int best_match( uint32_t a )
{
    int best_distance = INT32_MAX;
    int best_index = 0;

    for( int i = 0; i < 16; i++ )
    {
        int distance = color_distance(a, vga_base_colors[i]);
        if( distance < best_distance )
        {
            best_index = i;
            best_distance = distance;
        }
    }

    return best_index;
}

int is_wide( uint32_t codepoint )
{
    if( codepoint < 256 )
    {
        return 0;
    }
    return wcwidth( codepoint ) == 2;
}

// Utility function, output byte to serial port
static inline void outportb( unsigned short _port, unsigned char _data )
{
    asm volatile("outb %1, %0" :: "dN"(_port), "a"(_data));
}

static inline unsigned char inportb( unsigned short _port )
{
    unsigned char rv;
    asm volatile("inb %1, %0" : "=a"(rv):"dN"(_port));
    return rv;
}
