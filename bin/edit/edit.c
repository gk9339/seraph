#include <unistd.h>
#include <sys/termios.h>
#include <sys/ioctl.h>
#include <stdlib.h>
#include <string.h>
#include <ctype.h>
#include <stdio.h>
#include <errno.h>

#define VERSION "0.1"

#define CTRL_KEY(k) ((k) & 0x1f)

typedef struct edit_row
{
    int size;
    char* chars;
}edit_row;

struct config_struct
{
    int cx, cy;
    int term_rows;
    int term_cols;
    int numrows;
    edit_row* rows;
    struct termios prev_termios;
};
struct config_struct config;

struct abuf
{
    char* buf;
    int len;
};

enum editor_key
{
    ARROW_UP = 1000,
    ARROW_DOWN,
    ARROW_LEFT,
    ARROW_RIGHT,
    PAGE_UP,
    PAGE_DOWN,
    HOME_KEY,
    END_KEY,
    DEL_KEY
};

void enable_raw_mode( void );
void disable_raw_mode( void );

void process_keypress( void );
int read_key( void );
void move_cursor( int key );

void refresh_screen( void );
void draw_term_rows( struct abuf* ab );
void abuf_append( struct abuf* ab, const char* s, int len );

void open_file( char* filename );

int get_term_size( int* term_rows, int* term_cols );

int main( int argc, char** argv )
{
    enable_raw_mode();
    write(STDOUT_FILENO, "\033[H\033[2J", 7);

    config.cx = 0;
    config.cy = 0;
    config.numrows = 0;
    config.rows = NULL;
    get_term_size(&config.term_rows, &config.term_cols);

    if( argc >= 2 )
    {
        open_file(argv[1]);
    }

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
    int c = read_key();

    switch( c )
    {
        case CTRL_KEY('q'):
            write(STDOUT_FILENO, "\033[H\033[2J", 7);
            exit(0);
        case HOME_KEY:
            config.cx = 0;
            break;
        case END_KEY:
            config.cx = config.term_cols - 1;
            break;
        case PAGE_UP:
        case PAGE_DOWN:;
            int times = config.term_rows;
            while( times-- )
            {
                move_cursor( c == PAGE_UP? ARROW_UP : ARROW_DOWN );
            }
            break;
        case ARROW_UP:
        case ARROW_DOWN:
        case ARROW_LEFT:
        case ARROW_RIGHT:
            move_cursor(c);
            break;
        default:
            break;
    }
}

int read_key( void )
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

    if( c == '\033' )
    {
        char seq[3];
        if( read(STDIN_FILENO, &seq[0], 1) != 1 ) return '\033';
        if( read(STDIN_FILENO, &seq[1], 1) != 1 ) return '\033';

        if( seq[0] == '[' )
        {
            if( seq[1] >= '0' && seq[1] <= '9' )
            {
                if( read(STDIN_FILENO, &seq[2], 1) != 1 ) return '\033';
                if( seq[2] == '~' )
                {
                    switch( seq[1] )
                    {
                        case '1': return HOME_KEY;
                        case '3': return DEL_KEY;
                        case '4': return END_KEY;
                        case '5': return PAGE_UP;
                        case '6': return PAGE_DOWN;
                        case '7': return HOME_KEY;
                        case '8': return END_KEY;
                    }
                }
            }else
            {
                switch( seq[1] )
                {
                    case 'A': return ARROW_UP;
                    case 'B': return ARROW_DOWN;
                    case 'C': return ARROW_RIGHT;
                    case 'D': return ARROW_LEFT;
                    case 'H': return HOME_KEY;
                    case 'F': return END_KEY;
                }
            }
        }else if( seq[0] == '0' )
        {
            switch( seq[1] )
            {
                case 'H': return HOME_KEY;
                case 'F': return END_KEY;
            }
        }

        return '\033';
    }else
    {
        return c;
    }
}

void move_cursor( int key )
{
    switch( key )
    {
        case ARROW_UP:
            if( config.cy != 0 )
            {
                config.cy--;
            }
            break;
        case ARROW_DOWN:
            if( config.cy != config.term_rows - 1 )
            {
                config.cy++;
            }
            break;
        case ARROW_LEFT:
            if( config.cx != 0 )
            {
                config.cx--;
            }
            break;
        case ARROW_RIGHT:
            if( config.cx != config.term_cols - 1 )
            {
                config.cx++;
            }
            break;
    }
}

void refresh_screen( void )
{
    struct abuf ab = {NULL, 0};

    abuf_append(&ab, "\033[?25l\033[H", 9);

    draw_term_rows(&ab);
    
    char buf[32];
    snprintf(buf, sizeof(buf), "\033[%d;%dH", config.cy + 1, config.cx + 1);
    abuf_append(&ab, buf, strlen(buf));

    abuf_append(&ab, "\033[?25h", 6);

    write(STDOUT_FILENO, ab.buf, ab.len);
    
    free(ab.buf);
}

void draw_term_rows( struct abuf* ab )
{
    for( int y = 0; y < config.term_rows; y++ )
    {
        if( y >= config.numrows )
        {
            if( config.numrows == 0 && y == config.term_rows / 3 )
            {
                char welcome[80];
                int welcomelen = snprintf(welcome, sizeof(welcome), "seraph editor -- version %s", VERSION);
                if( welcomelen > config.term_cols ) 
                {
                    welcomelen = config.term_cols;
                }
                int padding = (config.term_cols - welcomelen) / 2;
                if( padding )
                {
                    abuf_append(ab, "~", 1);
                    padding--;
                }
        
                while(padding--)
                {
                    abuf_append(ab, " ", 1);
                }
        
                abuf_append(ab, welcome, welcomelen);
            }else if( config.numrows == 0 && y == (config.term_rows / 3) + 1 )
            {
                char welcome[80];
                int welcomelen = snprintf(welcome, sizeof(welcome), "CTRL+Q to quit");
                if( welcomelen > config.term_cols ) 
                {
                    welcomelen = config.term_cols;
                }
                int padding = (config.term_cols - welcomelen) / 2;
                if( padding )
                {
                    abuf_append(ab, "~", 1);
                    padding--;
                }
        
                while(padding--)
                {
                    abuf_append(ab, " ", 1);
                }
        
                abuf_append(ab, welcome, welcomelen);
            }else
            {
                abuf_append(ab, "~", 1);
            }
        }else
        {
            int len = config.rows[y].size;
            if( len > config.term_rows )
            {
                len = config.term_rows;
            }
            abuf_append(ab, config.rows[y].chars, len);
        }

        abuf_append(ab, "\033[K", 3);
        
        if( y < config.term_rows - 1 )
        {
            abuf_append(ab, "\n", 1);
        }
    }
}

void abuf_append( struct abuf* ab, const char* s, int len )
{
    char* new = realloc(ab->buf, ab->len + len);

    if( new == NULL )
    {
        return;
    }
    memcpy(&new[ab->len], s, len);
    ab->buf = new;
    ab->len += len;
}

void append_row( char* s, size_t len )
{
    config.rows = realloc(config.rows, sizeof(edit_row) * (config.numrows + 1));

    config.rows[config.numrows].size = len;
    config.rows[config.numrows].chars = malloc(len + 1);
    memcpy(config.rows[config.numrows].chars, s, len);
    config.rows[config.numrows].chars[len] = '\0';
    config.numrows++;
}

void open_file( char* filename )
{
    FILE* fp = fopen(filename, "r");
    if( !fp )
    {
        perror("fopen");
    }

    char* line = NULL;
    size_t linecap = 0;
    ssize_t linelen;

    while( (linelen = getline(&line, &linecap, fp)) != -1 )
    {
        while( linelen > 0 && (line[linelen - 1] == '\n' ||
                               line[linelen - 1] == '\r') )
        {
            linelen--;
        }
        append_row(line, linelen);
    }

    free(line);
    fclose(fp);
}

int get_term_size( int* term_rows, int* term_cols )
{
    struct winsize ws;

    if( ioctl(STDOUT_FILENO, TIOCGWINSZ, &ws) == -1 || ws.ws_col == 0)
    {
        return -1;
    }else
    {
        *term_rows = ws.ws_row;
        *term_cols = ws.ws_col;
        return 0;
    }
}
