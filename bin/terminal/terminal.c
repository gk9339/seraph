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
#include <sys/stat.h>
#include <wchar.h>
#include <libkbd/libkbd.h>
#include <libansiterm/libansiterm.h>
#include <debug.h>
#include <spinlock.h>
#include <pthread.h>
#include <kernel/lfb.h>
#include "pallete.h"
#include "decodeutf8.h"
#include "ununicode.h"
#include "terminal-font.h"

#define SWAP(T,a,b) { T _a = a; a = b; b = _a; }

int framebuffer_fd;
int framebuffer_width, framebuffer_height, framebuffer_stride;
char* framebuffer;

uint16_t terminal_width;
uint16_t terminal_height;
uint16_t csr_x = 0;
uint16_t csr_y = 0;
uint16_t _orig_csr_x = 0;
uint16_t _orig_csr_y = 0;
uint32_t fg = TERM_DEFAULT_FG;
uint32_t bg = TERM_DEFAULT_BG;
uint32_t _orig_fg = TERM_DEFAULT_FG;
uint32_t _orig_bg = TERM_DEFAULT_BG;
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

spin_lock_t input_buffer_lock = { 0 };
int input_buffer_semaphore[2];
list_t* input_buffer_queue = NULL;

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
void handle_input_string( char* ); // Write string into pty input buffer
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

void* handle_input_buffer( void* ); // When signalled, dequeue from input buffer, write to pty
void write_input_buffer( char* data, size_t len ); // Write character to pty input buffer
void handle_input( char ); // Write character to pty input buffer
void write_input_buffer( char*, size_t );
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

    framebuffer_fd = open("/dev/framebuffer", O_RDONLY);
    ioctl(framebuffer_fd, IO_LFB_WIDTH, &framebuffer_width);
    ioctl(framebuffer_fd, IO_LFB_HEIGHT, &framebuffer_height);
    ioctl(framebuffer_fd, IO_LFB_STRIDE, &framebuffer_stride);
    ioctl(framebuffer_fd, IO_LFB_ADDR, &framebuffer);
    close(framebuffer_fd);
    
    terminal_width = framebuffer_width / CHAR_WIDTH;
    terminal_height = framebuffer_height / CHAR_HEIGHT;

    // Open pty, setup pty slave FILE pointer
    openpty(&fd_master, &fd_slave, NULL, NULL, NULL);
    terminal = fdopen(fd_slave, "w");

    struct winsize pty_winsize;
    pty_winsize.ws_row = terminal_height;
    pty_winsize.ws_col = terminal_width;
    pty_winsize.ws_xpixel = 0;
    pty_winsize.ws_ypixel = 0;
    ioctl(fd_master, TIOCSWINSZ, &pty_winsize);

    // Initialize buffers and ANSI terminal state
    terminal_buffer_a = malloc(sizeof(term_cell_t) * terminal_width * terminal_height);
    memset(terminal_buffer_a, 0, sizeof(term_cell_t) * terminal_width * terminal_height);
    terminal_buffer_b = malloc(sizeof(term_cell_t) * terminal_width * terminal_height);
    memset(terminal_buffer_a, 0, sizeof(term_cell_t) * terminal_width * terminal_height);
    terminal_buffer = terminal_buffer_a;

    ansi_state = ansi_init(ansi_state, terminal_width, terminal_height, &terminal_callbacks);
    
    // Setup input buffer thread
    pthread_t input_buffer_thread;
    pipe(input_buffer_semaphore);
    input_buffer_queue = list_create();
    pthread_create(&input_buffer_thread, NULL, handle_input_buffer, NULL);

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

        tcsetpgrp(STDIN_FILENO, getpid());
     
        char* arg[] = { "/bin/sh", NULL };
        execvp("/bin/sh", arg);
    }else
    {
        // Parent process
        int kfd = open("/dev/kbd", O_RDONLY);
        int ret;
        char c;
     
        terminal_clear(2);

        // Clear anything in kbd buffer
        struct stat st;
        fstat(kfd, &st);
        for( size_t i = 0; i < st.st_size; i++ )
        {
            char tmp[1];
            read(kfd, tmp, 1);
        }

        int fds[] = {fd_master, kfd};
        char buf[BUFSIZ];
        key_event_state_t kbd_state = {0};
        key_event_t event;

        while( !exit_terminal )
        {
            // Wait on keyboard/pty master, or 200ms
            int res[] = {0, 0};
            fswait3(2, fds, 200, res);
            if( input_stopped ) continue;
    
            flip_cursor();
            if( res[0] )
            {
                // Read data out of pty master buffer and write it
                ret = read(fd_master, buf, BUFSIZ);
                for( int i = 0; i < ret; i++ )
                {
                    ansi_put(ansi_state, buf[i]);
                }
            }
            if( res[1] )
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

    char* message = "[Process completed]";
    for( size_t i = 0; i < strlen(message); i++ )
    {
        terminal_write(message[i]);
    }

    return EXIT_SUCCESS;
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
    exit_terminal = 1;
    signal(SIGCHLD, SIG_IGN);
    close(input_buffer_semaphore[1]);
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
            /* Bell is really annoying c:
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
            outportb(0x61, tmp);*/
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

// When signalled, dequeue from input buffer, write to pty
void* handle_input_buffer( void* args __attribute__((unused)) )
{
    while( 1 )
    {
        char tmp[1];
        int c = read(input_buffer_semaphore[0], tmp, 1);
        if( c > 0 )
        {
            spin_lock(input_buffer_lock);
            node_t* blob = list_dequeue(input_buffer_queue);
            spin_unlock(input_buffer_lock);

            if( !blob )
            {
                continue;
            }

            struct input_data* value = blob->value;
            write(fd_master, value->data, value->len);
            free(blob->value);
            free(blob);
        }else
        {
            break;
        }
    }

    close(input_buffer_semaphore[0]);
    return NULL;
}

// Write data to input buffer queue, signal input buffer thread
void write_input_buffer( char* data, size_t len )
{
    struct input_data* input_data = malloc(sizeof(struct input_data) + len);
    input_data->len = len;
    memcpy(&input_data->data, data, len);

    spin_lock(input_buffer_lock);
    list_insert(input_buffer_queue, input_data);
    spin_unlock(input_buffer_lock);

    write(input_buffer_semaphore[1], input_data, 1);
}

// Write character to pty input buffer
void handle_input( char c )
{
    write_input_buffer(&c, 1);
}

// Write string into pty input buffer
void handle_input_string( char* c )
{
    write_input_buffer(c, strlen(c));
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

static void term_set_point(int x, int y, uint32_t value)
{
    uint32_t * disp = (uint32_t *)framebuffer;
    uint32_t * cell = &disp[y * (framebuffer_stride / 4) + x];
    *cell = value;
}

void terminal_write_char( uint32_t val, uint16_t x, uint16_t y, uint32_t c_fg, uint32_t c_bg, uint8_t flags __attribute__((unused)) )
{
    c_fg = term_colors[c_fg];
    c_fg |= 0xFF << 24;
    c_bg = term_colors[c_bg];
    c_bg |= TERM_DEFAULT_OPAC << 24;
    if( val > 128 )
    {
        val = ununicode(val);
    }

    x *= CHAR_WIDTH;
    y *= CHAR_HEIGHT;

    uint16_t* c = large_font[val];
    for( uint8_t i = 0; i < CHAR_HEIGHT; ++i )
    {
        for( uint8_t j = 0; j < CHAR_WIDTH; ++j )
        {
            if( (c[i] & (1 << (15-j))) )
            {
                term_set_point(x+j,y+i,c_fg);
            }else
            {
                term_set_point(x+j,y+i,c_bg);
            }
        }
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
