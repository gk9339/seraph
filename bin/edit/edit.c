#include <unistd.h>
#include <sys/termios.h>
#include <sys/ioctl.h>
#include <stdlib.h>
#include <string.h>
#include <ctype.h>
#include <stdio.h>
#include <errno.h>
#include <time.h>
#include <stdarg.h>
#include <fcntl.h>

#define VERSION "0.1"
#define TABSTOP 4

#define CTRL_KEY(k) ((k) & 0x1f)

typedef struct edit_row
{
    int size;
    int rsize;
    char* chars;
    char* render;
}edit_row;

struct config_struct
{
    int cx, cy;
    int rx;
    int scroll_rows;
    int scroll_cols;
    int term_rows;
    int term_cols;
    int numrows;
    edit_row* rows;
    int dirty;
    char* filename;
    char statusmsg[80];
    time_t statusmsg_time;
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
    BACKSPACE = 127,
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
void error( const char* s );

void process_keypress( void );
int read_key( void );
void move_cursor( int key );
char* status_prompt( char* prompt, void(*callback)( char*, int ) );

void refresh_screen( void );
void draw_term_rows( struct abuf* ab );
void draw_term_status_bar( struct abuf* ab );
void draw_term_message_bar( struct abuf* ab );
void set_statusmsg( const char* fmt, ... );
void scroll( void );
void abuf_append( struct abuf* ab, const char* s, int len );
int cx_to_rx( edit_row* row, int cx );
void row_insert_char( edit_row* row, int at, int c );
void row_del_char( edit_row* row, int at );
void insert_char( int c );
void insert_newline( void );
void del_char( void );

void open_file( char* filename );
void save_file( void );
char* rows_to_string( int* buflen );
void find( void );
void find_callback( char* query, int key );

int get_term_size( int* term_rows, int* term_cols );

int main( int argc, char** argv )
{
    enable_raw_mode();
    write(STDOUT_FILENO, "\033[H\033[2J", 7);

    config.cx = 0;
    config.cy = 0;
    config.rx = 0;
    config.scroll_rows = 0;
    config.scroll_cols = 0;
    config.numrows = 0;
    config.rows = NULL;
    config.dirty = 0;
    config.filename = NULL;
    config.statusmsg[0] = '\0';
    config.statusmsg_time = 0;
    get_term_size(&config.term_rows, &config.term_cols);
    config.term_rows -= 2;

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

void error( const char* s )
{
    write(STDOUT_FILENO, "\033[H\033[2J", 7);

    perror(s);
    exit(1);
}

void process_keypress( void )
{
    static int quit_tries = 1;
    int c = read_key();

    switch( c )
    {
        case '\r':
            insert_newline();
            break;
        case CTRL_KEY('q'):
            if( config.dirty && quit_tries > 0 )
            {
                set_statusmsg("UNSAVED CHANGES: ^Q again to quit");
                quit_tries--;
                return;
            }
            write(STDOUT_FILENO, "\033[H\033[2J", 7);
            exit(0);
        case CTRL_KEY('s'):
            save_file();
            break;
        case HOME_KEY:
            config.cx = 0;
            break;
        case END_KEY:
            if( config.cy < config.numrows )
            {
                config.cx = config.rows[config.cy].size;
            }
            break;
        case CTRL_KEY('f'):
            find();
            break;
        case BACKSPACE:
        case CTRL_KEY('h'):
        case DEL_KEY:
            if( c == DEL_KEY )
            {
                move_cursor(ARROW_RIGHT);
            }
            del_char();
            break;
        case PAGE_UP:
        case PAGE_DOWN:
            if( c == PAGE_UP )
            {
                config.cy = config.scroll_rows;
            }else if( c == PAGE_DOWN )
            {
                config.cy = config.scroll_rows + config.term_rows - 1;
                if( config.cy > config.numrows )
                {
                    config.cy = config.numrows;
                }
            }

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
        case CTRL_KEY('l'):
        case '\033':
            break;
        default:
            insert_char(c);
            break;
    }

    quit_tries = 1;
}

int read_key( void )
{
    int nread;
    char c;

    while( (nread = read(STDIN_FILENO, &c, 1)) != 1 )
    {
        if( nread == -1 && errno != EAGAIN )
        {
            error("read");
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
    edit_row* row = (config.cy >= config.numrows)? NULL : &config.rows[config.cy];

    switch( key )
    {
        case ARROW_UP:
            if( config.cy != 0 )
            {
                config.cy--;
            }
            break;
        case ARROW_DOWN:
            if( config.cy < config.numrows )
            {
                config.cy++;
            }
            break;
        case ARROW_LEFT:
            if( config.cx != 0 )
            {
                config.cx--;
            }else if( config.cy > 0 )
            {
                config.cy--;
                config.cx = config.rows[config.cy].size;
            }
            break;
        case ARROW_RIGHT:
            if( row && config.cx < row->size )
            {
                config.cx++;
            }else if( row && config.cx == row->size )
            {
                config.cy++;
                config.cx = 0;
            }
            break;
    }

    row = (config.cy >= config.numrows)? NULL : &config.rows[config.cy];
    int rowlen = row? row->size : 0;
    if( config.cx > rowlen )
    {
        config.cx = rowlen;
    }
}

char* status_prompt( char* prompt, void(*callback)( char*, int ) )
{
    size_t bufsize = 128;
    char* buf = malloc(bufsize);

    size_t buflen = 0;
    buf[0] = '\0';

    while( 1 )
    {
        set_statusmsg(prompt, buf);
        refresh_screen();

        int c = read_key();
        if( c == DEL_KEY || c == CTRL_KEY('h') || c == BACKSPACE )
        {
            if( buflen != 0 )
            {
                buf[--buflen] = '\0';
            }
        }else if( c == '\033' )
        {
            set_statusmsg("");
            if( callback )
            {
                callback(buf, c);
            }
            free(buf);
            return NULL;
        }else if( c == '\r' )
        {
            if( buflen != 0 )
            {
                set_statusmsg("");
                if( callback )
                {
                    callback(buf, c);
                }
                return buf;
            }
        }else if( !iscntrl(c) && c < 128 )
        {
            if( buflen == bufsize - 1 )
            {
                bufsize *= 2;
                buf = realloc(buf, bufsize);
            }
            buf[buflen++] = c;
            buf[buflen] = '\0';
        }

        if( callback )
        {
            callback(buf, c);
        }
    }
}

void refresh_screen( void )
{
    struct abuf ab = {NULL, 0};

    scroll();

    abuf_append(&ab, "\033[?25l\033[H", 9);

    draw_term_rows(&ab);
    draw_term_status_bar(&ab);
    draw_term_message_bar(&ab);

    char buf[32];
    snprintf(buf, sizeof(buf), "\033[%d;%dH", (config.cy - config.scroll_rows) + 1, (config.rx - config.scroll_cols) + 1);
    abuf_append(&ab, buf, strlen(buf));

    abuf_append(&ab, "\033[?25h", 6);

    write(STDOUT_FILENO, ab.buf, ab.len);
    
    free(ab.buf);
}

void draw_term_rows( struct abuf* ab )
{
    for( int y = 0; y < config.term_rows; y++ )
    {
        int file_row = y + config.scroll_rows;
        if( file_row >= config.numrows )
        {
            if( config.numrows == 0 && y == config.term_rows / 3 )
            {
                char welcome[80];
                int welcomelen = snprintf(welcome, sizeof(welcome), "\033[1;36mseraph\033[0;m editor -- version %s", VERSION);
                if( welcomelen > config.term_cols ) 
                {
                    welcomelen = config.term_cols;
                }
                int padding = (config.term_cols - (welcomelen - 12)) / 2;
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
            int len = config.rows[file_row].rsize - config.scroll_cols;
            if( len < 0 )
            {
                len = 0;
            }
            if( len > config.term_cols )
            {
                len = config.term_cols;
            }
            abuf_append(ab, &config.rows[file_row].render[config.scroll_cols], len);
        }

        abuf_append(ab, "\033[K", 3);   
        abuf_append(ab, "\r\n", 2);
    }
}

void draw_term_status_bar( struct abuf* ab )
{
    abuf_append(ab, "\033[7m", 4);

    char status[80], rstatus[80];
    int len = snprintf(status, sizeof(status), "%.20s - %d lines %s", config.filename? config.filename : "[No Name]", config.numrows, config.dirty? "(modified)" : "");
    int rlen = snprintf(rstatus, sizeof(rstatus), "%d/%d", config.cy + 1, config.numrows);

    if( len > config.term_cols )
    {
        len = config.term_cols;
    }

    abuf_append(ab, status, len);

    while( len < config.term_cols )
    {
        if( config.term_cols - len == rlen )
        {
            abuf_append(ab, rstatus, rlen);
            break;
        }else
        {
            abuf_append(ab, " ", 1);
            len++;
        }
    }

    abuf_append(ab, "\033[m\r\n", 5);
}

void draw_term_message_bar( struct abuf* ab )
{
    abuf_append(ab, "\033[K", 3);

    int msglen = strlen(config.statusmsg);
    if( msglen > config.term_cols )
    {
        msglen = config.term_cols;
    }

    if( msglen && time(NULL) - config.statusmsg_time < 5 )
    {
        abuf_append(ab, config.statusmsg, msglen);
    }else
    {
        abuf_append(ab, "^Q - Quit | ^S - Save | ^F - Find", 34);
    }
}

void set_statusmsg( const char* fmt, ... )
{
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(config.statusmsg, sizeof(config.statusmsg), fmt, ap);
    va_end(ap);
    config.statusmsg_time = time(NULL);
}

void scroll( void )
{
    config.rx = 0;
    if( config.cy < config.numrows )
    {
        config.rx = cx_to_rx(&config.rows[config.cy], config.cx);
    }

    if( config.cy < config.scroll_rows )
    {
        config.scroll_rows = config.cy;
    }

    if( config.cy >= config.scroll_rows + config.term_rows )
    {
        config.scroll_rows = config.cy - config.term_rows + 1;
    }

    if( config.rx < config.scroll_cols )
    {
        config.scroll_cols = config.rx;
    }

    if( config.rx >= config.scroll_cols + config.term_cols )
    {
        config.scroll_cols = config.rx - config.term_cols + 1;
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

int cx_to_rx( edit_row* row, int cx )
{
    int rx = 0;
    int j;
    for( j = 0; j < cx; j++ )
    {
        if( row->chars[j] == '\t' )
        {
            rx += (TABSTOP - 1) - (rx % TABSTOP);
        }
        rx++;
    }

    return rx;
}

int rx_to_cx( edit_row* row, int rx )
{
    int cur_rx = 0;
    int cx;
    for( cx = 0; cx < row ->size; cx++ )
    {
        if( row->chars[cx] == '\t' )
        {
            cur_rx += (TABSTOP - 1)  - (cur_rx % TABSTOP);
        }
        cur_rx++;

        if( cur_rx > rx )
        {
            return cx;
        }
    }

    return cx;
}

void update_row( edit_row* row )
{
    int tabs = 0;
    int j;

    for( j = 0; j < row->size; j++ )
    {
        if( row-> chars[j] == '\t' )
        {
            tabs++;
        }
    }

    free(row->render);
    row->render = malloc(row->size + tabs * (TABSTOP - 1) + 1);

    int idx = 0;

    for( j = 0; j < row->size; j++ )
    {
        if( row->chars[j] == '\t' )
        {
            row->render[idx++] = ' ';
            while( idx % TABSTOP != 0 )
            {
                row ->render[idx++] = ' ';
            }
        }else
        {
            row->render[idx++] = row->chars[j];
        }
    }

    row->render[idx] = '\0';
    row->rsize = idx;
}

void insert_row( int at, char* s, size_t len )
{
    if( at < 0 || at > config.numrows )
    {
        return;
    }

    config.rows = realloc(config.rows, sizeof(edit_row) * (config.numrows + 1));
    memmove(&config.rows[at + 1], &config.rows[at], sizeof(edit_row) * (config.numrows - at));

    config.rows[at].size = len;
    config.rows[at].chars = malloc(len + 1);
    memcpy(config.rows[at].chars, s, len);
    config.rows[at].chars[len] = '\0';

    config.rows[at].rsize = 0;
    config.rows[at].render = NULL;
    update_row(&config.rows[at]);

    config.numrows++;
    config.dirty++;
}

void free_row( edit_row* row )
{
    free(row->render);
    free(row->chars);
}

void del_row( int at )
{
    if( at < 0 || at >= config.numrows )
    {
        return;
    }
    free_row(&config.rows[at]);
    memmove(&config.rows[at], &config.rows[at+1], sizeof(edit_row) * (config.numrows - at - 1));
    config.numrows--;
    config.dirty++;
}

void row_insert_char( edit_row* row, int at, int c )
{
    if( at < 0 || at > row->size )
    {
        at = row->size;
    }

    row->chars = realloc(row->chars, row->size + 2);
    memmove(&row->chars[at+1], &row->chars[at], row->size - at + 1);

    row->size++;
    row->chars[at] = c;

    update_row(row);
    config.dirty++;
}

void row_append_string( edit_row* row, char* s, size_t len )
{
    row->chars = realloc(row->chars, row->size + len + 1);
    memcpy(&row->chars[row->size], s, len);
    row->size += len;
    row->chars[row->size] = '\0';
    update_row(row);

    config.dirty++;
}

void row_del_char( edit_row* row, int at )
{
    if( at < 0 || at >= row->size )
    {
        return;
    }

    memmove(&row->chars[at], &row->chars[at + 1], row->size - at);
    row->size--;
    update_row(row);

    config.dirty++;
}

void insert_char( int c )
{
    if( config.cy == config.numrows )
    {
        insert_row(config.numrows, "", 0);
    }

    row_insert_char(&config.rows[config.cy], config.cx, c);
    config.cx++;
}

void insert_newline()
{
    if( config.cx == 0 )
    {
        insert_row(config.cy, "", 0);
    }else
    {
        edit_row* row = &config.rows[config.cy];
        insert_row(config.cy + 1, &row->chars[config.cx], row->size - config.cx);
        row = &config.rows[config.cy];
        row->size = config.cx;
        row->chars[row->size] = '\0';
        update_row(row);
    }
    config.cy++;
    config.cx = 0;
}

void del_char()
{
    if( config.cy == config.numrows )
    {
        return;
    }
    if( config.cx == 0 && config.cy == 0 )
    {
        return;
    }

    edit_row* row = &config.rows[config.cy];
    if( config.cx > 0 )
    {
        row_del_char(row, config.cx - 1);
        config.cx--;
    }else
    {
        config.cx = config.rows[config.cy - 1].size;
        row_append_string(&config.rows[config.cy - 1], row->chars, row->size);
        del_row(config.cy);
        config.cy--;
    }
}

void open_file( char* filename )
{
    free(config.filename);
    config.filename = strdup(filename);

    FILE* fp = fopen(filename, "r");
    if( !fp )
    {
        error("fopen");
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
        insert_row(config.numrows, line, linelen);
    }

    free(line);
    fclose(fp);

    config.dirty = 0;
}

void save_file( void )
{
    if( config.filename == NULL )
    {
        config.filename = status_prompt("Save as: %s", NULL);
        if( config.filename == NULL )
        {
            set_statusmsg("Save cancelled");
            return;
        }
    }

    int len;
    char* buf = rows_to_string(&len);

    int fd = open(config.filename, O_RDWR | O_CREAT | O_TRUNC, 0644);
    if( fd != -1 )
    {
        write(fd, buf, len);
        close(fd);
        config.dirty = 0;
        set_statusmsg("%d bytes written", len);
    }else
    {
        set_statusmsg("I/O error: %s", strerror(errno));
    }

    free(buf);
}

char* rows_to_string( int* buflen )
{
    int len = 0;
    int j;
    for( j = 0; j < config.numrows; j++ )
    {
        len += config.rows[j].size + 1;
    }
    *buflen = len;

    char* buf = malloc(len);
    char* p = buf;
    for( j = 0; j < config.numrows; j++ )
    {
        memcpy(p, config.rows[j].chars, config.rows[j].size);
        p += config.rows[j].size;
        *p = '\n';
        p++;
    }

    return buf;
}

void find( void )
{
    int save_cx = config.cx;
    int save_cy = config.cy;
    int save_scroll_rows = config.scroll_rows;
    int save_scroll_cols = config.scroll_cols;

    char* query = status_prompt("Search: %s", find_callback);
    
    if( query )
    {
        free(query);
    }else
    {
        config.cx = save_cx;
        config.cy = save_cy;
        config.scroll_rows = save_scroll_rows;
        config.scroll_cols = save_scroll_cols;
    }
}

void find_callback( char* query, int key )
{
    static int last_match = -1;
    static int direction = 1;

    if( key == '\r' || key == '\033' )
    {
        last_match = -1;
        direction = 1;
        return;
    }else if( key == ARROW_RIGHT || key == ARROW_DOWN )
    {
        direction = 1;
    }else if( key == ARROW_LEFT || key == ARROW_UP)
    {
        direction = -1;
    }else
    {
        last_match = -1;
        direction = 1;
    }

    if( last_match == -1 )
    {
        direction = 1;
    }
    int current = last_match;
    int i;
    for( i = 0; i < config.numrows; i++ )
    {
        current += direction;
        if( current == -1 )
        {
            current = config.numrows - 1;
        }else if( current == config.numrows )
        {
            current = 0;
        }

        edit_row* row = &config.rows[current];
        char* match = strstr(row->render, query);
        if( match )
        {
            last_match = current;
            config.cy = current;
            config.cx = rx_to_cx(row, match - row->render);
            config.scroll_rows = config.numrows;
            break;
        }
    }
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
