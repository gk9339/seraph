#include <unistd.h>
#include <sys/termios.h>
#include <sys/ioctl.h>
#include <stdlib.h>
#include <ctype.h>
#include <stdio.h>
#include <errno.h>

#define CTRL_KEY(k) ((k) & 0x1f)

struct config_struct
{
    int cx, cy;
    int rows;
    int cols;
    struct termios prev_termios;
};
struct config_struct config;

void disable_raw_mode( void );
void enable_raw_mode( void );

void process_keypress( void );
char read_key( void );

void refresh_screen( void );
void draw_rows( void );

int get_term_size( int* rows, int* cols );

int main( void )
{
    char c;

    enable_raw_mode();
    write(STDOUT_FILENO, "\033[H\033[2J", 7);
    get_term_size(&config.rows, &config.cols);

    while( 1 )
    {
        refresh_screen();
        process_keypress();
    }

    return 0;
}

void enable_raw_mode( void )
{
    struct termios new_termios;

    tcgetattr(STDIN_FILENO, &config.prev_termios);
    new_termios = config.prev_termios;
    atexit(disable_raw_mode);

    new_termios.c_iflag &= ~(ICRNL | IXON);
    new_termios.c_lflag &= ~(ECHO | ICANON | ISIG);

    tcsetattr(STDIN_FILENO, TCSAFLUSH, &new_termios);
}

void disable_raw_mode( void )
{
    tcsetattr(STDIN_FILENO, TCSAFLUSH, &config.prev_termios);
}

void process_keypress( void )
{
    char c = read_key();

    switch( c )
    {
        case CTRL_KEY('q'):
            write(STDOUT_FILENO, "\033[H\033[2J", 7);
            exit(0);
        default:
            break;
    }
}

char read_key( void )
{
    int nread;
    char c;

    while( (nread = read(STDIN_FILENO, &c, 1)) != 1 )
    {
        if( nread == -1 && errno != EAGAIN )
        {
            perror("read");
            exit(1);
        }
    }

    return c;
}

void refresh_screen( void )
{
    write(STDOUT_FILENO, "\033[H\033[2J", 7);

    draw_rows();
    
    write(STDOUT_FILENO, "\033[H", 3);
}

void draw_rows( void )
{
    for( int y = 0; y < config.rows; y++ )
    {
        write(STDOUT_FILENO, "~ \n", 3);
    }
}

int get_term_size( int* rows, int* cols )
{
    struct winsize ws;

    if( ioctl(STDOUT_FILENO, TIOCGWINSZ, &ws) == -1 || ws.ws_col == 0)
    {
        return -1;
    }else
    {
        *rows = ws.ws_row;
        *cols = ws.ws_col;
        return 0;
    }
}
