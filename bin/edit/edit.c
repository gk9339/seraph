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

#define VERSION "0.2"
#define TABSTOP 4

#define CTRL_KEY(k) ((k) & 0x1f)

#define HL_STRINGS (1<<0)
#define HL_NUMBERS (1<<1)
#define HL_MACROS  (1<<2)

typedef struct edit_row
{
    int idx;
    int size;
    int rsize;
    char* chars;
    char* render;
    unsigned char* highlight;
    int highlight_open_comment;
}edit_row;

struct edit_struct
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
    int newfile;
    char* filename;
    char statusmsg[80];
    time_t statusmsg_time;
    struct syntax* syntax;
    struct termios prev_termios;
};
struct edit_struct edit;

struct edit_buf
{
    char* buf;
    int len;
};

struct syntax
{
    char* filetype;
    char** filematch;
    char** keywords;
    char** macros;
    char* single_line_comment_start;
    char* multi_line_comment_start;
    char* multi_line_comment_end;
    int flags;
};

char* C_HL_extensions[] = { ".c", ".h", ".cpp", NULL };
char* C_HL_keywords[] = { "auto", "break", "case", "const", "continue", "default", "do", "else",
                          "extern", "for", "goto", "if", "return", "sizeof", "switch", "typedef", 
                          "while", "union", "struct", "enum",
                          "char|", "signed|", "unsigned|", "short|", "int|", "long|", "float|", 
                          "double|", "bool|", "_Bool|", "size_t|", "ssize_t|", "ptrdiff_t|", 
                          "uint8_t|", "uint16_t|", "uint32_t|", "uint64_t|", "int8_t|", "int16_t|",
                          "int32_t|", "int64_t|", "uintptr_t|", "intptr_t|", "uintmax_t|",
                          "intmax_t|", "wint_t|", "void|", "static|", "volatile|", "register|",
                          NULL };
char* C_HL_macros[] = { "#include", "#pragma", "#define", "#error", "#warning", "#undef", "#if",
                        "#else", "#elif", "#endif", "#elif", "#ifdef", "#ifndef", "#line", NULL };

char* PY_HL_extensions[] = { ".py", NULL };
char* PY_HL_keywords[] = { "and", "as", "assert", "break", "class", "continue", "def", "del", "elif", 
                           "else", "except", "exec", "finally", "for", "from", "global","if",
                           "import", "in", "is", "lambda", "not", "or", "pass", "print", "raise",
                           "return", "try", "while", "with", "yield", "async", "await", "nonlocal",
                           "range", "xrange", "reduce", "map", "filter", "all", "any", "sum", "dir",
                           "abs", "breakpoint", "compile", "delattr", "divmod", "format", "eval",
                           "getattr", "hasattr", "hash", "help", "id", "input", "isinstance",
                           "issubclass", "len", "locals", "max", "min", "next", "open", "pow", "repr",
                           "reversed", "round", "setattr", "slice", "sorted", "super", "vars", "zip",
                           "__import__", "reload", "raw_input", "execfile", "file", "cmp", "basestring",
                           "buffer|", "bytearray|", "bytes|", "complex|", "float|", "frozenset|",
                           "int|", "list|", "long|", "None|", "set|", "str|", "chr|", "tuple|",
                           "bool|", "False|", "True|", "type|", "unicode|", "dict|", "ascii|", "bin|",
                           "callable|", "classmethod|", "enumerate|", "hex|", "oct|", "ord|", "iter|",
                           "memoryview|", "object|", "property|", "staticmethod|", "unichr|", NULL };

char* SH_HL_extensions[] = { ".sh", NULL };
char* SH_HL_keywords[] = { "echo", "read", "set", "unset", "readonly", "shift", "export", "if", "fi",
                           "else", "while", "do", "done", "for", "until", "case", "esac", "break",
                           "continue", "exit", "return", "trap", "wait", "eval", "exec", "ulimit",
                           "umask", NULL };

struct syntax HLDB[] =
{
    {
        "c",
        C_HL_extensions,
        C_HL_keywords,
        C_HL_macros,
        "//", "/*", "*/",
        HL_STRINGS | HL_NUMBERS | HL_MACROS
    },
    {
        "python",
        PY_HL_extensions,
        PY_HL_keywords,
        NULL,
        "#", NULL, NULL,
        HL_STRINGS | HL_NUMBERS
    },
    {
        "shell",
        SH_HL_extensions,
        SH_HL_keywords,
        NULL,
        "#", NULL, NULL,
        HL_STRINGS | HL_NUMBERS
    },
};

#define HLDB_ENTRIES (sizeof(HLDB) / sizeof(HLDB[0]))

enum edit_key
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

enum edit_highlight
{
    HL_NORMAL,
    HL_COMMENT,
    HL_MLCOMMENT,
    HL_KEYWORD1,
    HL_KEYWORD2,
    HL_MACRO,
    HL_STRING,
    HL_NUMBER,
    HL_MATCH
};

// Initialization / Shutdown
void init( void );
int get_term_size( int* term_rows, int* term_cols );
void enable_raw_mode( void );
void disable_raw_mode( void );
void error( const char* s );

// Input
void process_keypress( void );
int read_key( void );
void move_cursor( int key );
char* status_bar_prompt( char* prompt, void(*callback)( char*, int ) );

// Output
void refresh_screen( void );
void draw_text_rows( struct edit_buf* eb );
void draw_status_bar( struct edit_buf* eb );
void draw_message_bar( struct edit_buf* eb );
void set_statusmsg( const char* fmt, ... );
void scroll( void );
void edit_buf_append( struct edit_buf* eb, const char* s, int len );
int cx_to_rx( edit_row* row, int cx );
int rx_to_cx( edit_row* row, int rx );
void update_row( edit_row* row );
void insert_row( int at, char* s, size_t len );
void free_row( edit_row* row );
void del_row( int at );
void row_insert_char( edit_row* row, int at, int c );
void row_append_string( edit_row* row, char* s, size_t len );
void row_del_char( edit_row* row, int at );
void insert_char( int c );
void insert_newline( void );
void del_char( void );

// File I/O
void open_file( char* filename );
void save_file( void );
char* rows_to_string( int* buflen );

// Search
void find( void );
void find_callback( char* query, int key );

// Syntax Highlighting
void update_syntax( edit_row* row );
int row_has_open_comment( edit_row* row );
int syntax_to_color( int highlight );
int is_seperator( int c );
void syntax_from_file_extension( void );

int main( int argc, char** argv )
{
    enable_raw_mode();
    init();

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

void init( void )
{
    write(STDOUT_FILENO, "\033[H\033[2J", 7);

    edit.cx = 0;
    edit.cy = 0;
    edit.rx = 0;
    edit.scroll_rows = 0;
    edit.scroll_cols = 0;
    edit.numrows = 0;
    edit.rows = NULL;
    edit.dirty = 0;
    edit.filename = NULL;
    edit.statusmsg[0] = '\0';
    edit.statusmsg_time = 0;
    edit.syntax = NULL;
    get_term_size(&edit.term_rows, &edit.term_cols);
    edit.term_rows -= 2;
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

void enable_raw_mode( void )
{
    struct termios new_termios;

    tcgetattr(STDIN_FILENO, &edit.prev_termios);
    new_termios = edit.prev_termios;
    atexit(disable_raw_mode);

    new_termios.c_iflag &= ~(ICRNL | IXON);
    new_termios.c_lflag &= ~(ECHO | ICANON | ISIG);

    tcsetattr(STDIN_FILENO, TCSAFLUSH, &new_termios);
}

void disable_raw_mode( void )
{
    tcsetattr(STDIN_FILENO, TCSAFLUSH, &edit.prev_termios);
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
            if( edit.dirty && quit_tries > 0 )
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
            edit.cx = 0;
            break;
        case END_KEY:
            if( edit.cy < edit.numrows )
            {
                edit.cx = edit.rows[edit.cy].size;
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
                edit.cy = edit.scroll_rows;
            }else if( c == PAGE_DOWN )
            {
                edit.cy = edit.scroll_rows + edit.term_rows - 1;
                if( edit.cy > edit.numrows )
                {
                    edit.cy = edit.numrows;
                }
            }

            int times = edit.term_rows;
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
        case CTRL_KEY('j'):
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
    edit_row* row = (edit.cy >= edit.numrows)? NULL : &edit.rows[edit.cy];

    switch( key )
    {
        case ARROW_UP:
            if( edit.cy != 0 )
            {
                edit.cy--;
            }
            break;
        case ARROW_DOWN:
            if( edit.cy < edit.numrows - 1 )
            {
                edit.cy++;
            }
            break;
        case ARROW_LEFT:
            if( edit.cx != 0 )
            {
                edit.cx--;
            }else if( edit.cy > 0 )
            {
                edit.cy--;
                edit.cx = edit.rows[edit.cy].size;
            }
            break;
        case ARROW_RIGHT:
            if( row && edit.cx < row->size )
            {
                edit.cx++;
            }else if( row && edit.cx == row->size && edit.cy != edit.numrows - 1 )
            {
                edit.cy++;
                edit.cx = 0;
            }
            break;
    }

    row = (edit.cy >= edit.numrows)? NULL : &edit.rows[edit.cy];
    int rowlen = row? row->size : 0;
    if( edit.cx > rowlen )
    {
        edit.cx = rowlen;
    }
}

char* status_bar_prompt( char* prompt, void(*callback)( char*, int ) )
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
    struct edit_buf eb = {NULL, 0};

    scroll();

    edit_buf_append(&eb, "\033[?25l\033[H", 9);

    draw_text_rows(&eb);
    draw_status_bar(&eb);
    draw_message_bar(&eb);

    char buf[32];
    snprintf(buf, sizeof(buf), "\033[%d;%dH", (edit.cy - edit.scroll_rows) + 1, (edit.rx - edit.scroll_cols) + 1);
    edit_buf_append(&eb, buf, strlen(buf));

    edit_buf_append(&eb, "\033[?25h", 6);

    write(STDOUT_FILENO, eb.buf, eb.len);
    
    free(eb.buf);
}

void draw_text_rows( struct edit_buf* eb )
{
    for( int y = 0; y < edit.term_rows; y++ )
    {
        int file_row = y + edit.scroll_rows;
        if( file_row >= edit.numrows )
        {
            if( edit.numrows == 0 && y == edit.term_rows / 3 )
            {
                char welcome[80];
                int welcomelen = snprintf(welcome, sizeof(welcome), "\033[1;36mseraph\033[0;m editor -- version %s", VERSION);
                if( welcomelen > edit.term_cols ) 
                {
                    welcomelen = edit.term_cols;
                }
                int padding = (edit.term_cols - (welcomelen - 12)) / 2;
                if( padding )
                {
                    edit_buf_append(eb, "\033[38;5;33m~\033[0;m", 16);
                    padding--;
                }
        
                while(padding--)
                {
                    edit_buf_append(eb, " ", 1);
                }
        
                edit_buf_append(eb, welcome, welcomelen);
            }else if( y != 0 )
            {
                edit_buf_append(eb, "\033[38;5;33m~\033[0;m", 16);
            }
        }else
        {
            int len = edit.rows[file_row].rsize - edit.scroll_cols;
            if( len < 0 )
            {
                len = 0;
            }
            if( len > edit.term_cols )
            {
                len = edit.term_cols;
            }
            
            char* c = &edit.rows[file_row].render[edit.scroll_cols];
            unsigned char* hl = &edit.rows[file_row].highlight[edit.scroll_cols];
            int current_color = -1;
            int j;
            for( j = 0; j < len; j++ )
            {
                if( iscntrl(c[j]) )
                {
                    char sym = (c[j] < 26)? '@' + c[j] : '?';
                    edit_buf_append(eb, "\033[7m", 4);
                    edit_buf_append(eb, &sym, 1);
                    edit_buf_append(eb, "\033[m", 3);
                    if( current_color != -1 )
                    {
                        char buf[16];
                        int clen = snprintf(buf, sizeof(buf), "\033[%dm", current_color);
                        edit_buf_append(eb, buf, clen);
                    }
                }else if( hl[j] == HL_NORMAL )
                {
                    if( current_color != -1 )
                    {
                        edit_buf_append(eb, "\033[0m", 4);
                        current_color = -1;
                    }
                    edit_buf_append(eb, &c[j], 1);
                }else
                {
                    int color = syntax_to_color(hl[j]);
                    if( color != current_color )
                    {
                        char buf[16];
                        int clen = snprintf(buf, sizeof(buf), "\033[0m\033[%dm", color);
                        edit_buf_append(eb, buf, clen);
                        current_color = color;
                    }
                    edit_buf_append(eb, &c[j], 1);
                }
            }
            edit_buf_append(eb, "\033[0m", 4);
        }

        edit_buf_append(eb, "\033[K", 3);   
        edit_buf_append(eb, "\r\n", 2);
    }
}

void draw_status_bar( struct edit_buf* eb )
{
    edit_buf_append(eb, "\033[48;5;252m", 11);
    edit_buf_append(eb, "\033[30m", 5);

    char status[80], r1status[80], r2status[80], r3status[80];
    int len = snprintf(status, sizeof(status), " %.20s - %d lines%s%s ", edit.filename? edit.filename : "[No Name]", edit.numrows, edit.newfile? " [New]" : "", edit.dirty? " [Modified]" : "");
    int r1len = snprintf(r1status, sizeof(r1status), " %s ", edit.syntax? edit.syntax->filetype : "text");
    int r2len = snprintf(r2status, sizeof(r2status), " %.0f%% ", edit.numrows != 0? ((float)(edit.cy + 1) / (float)edit.numrows)*100 : 100);
    int r3len = snprintf(r3status, sizeof(r3status), " %d/%d ", edit.cy + 1, edit.numrows);

    if( len > edit.term_cols )
    {
        len = edit.term_cols;
    }

    edit_buf_append(eb, status, len);
    edit_buf_append(eb, "\033[48;5;240m", 11);
    while( len < edit.term_cols )
    {
        if( edit.term_cols - len == r1len + r2len + r3len )
        {
            edit_buf_append(eb, "\033[48;5;248m", 11);
            edit_buf_append(eb, r1status, r1len);
            edit_buf_append(eb, "\033[48;5;250m", 11);
            edit_buf_append(eb, r2status, r2len);
            edit_buf_append(eb, "\033[48;5;254m", 11);
            edit_buf_append(eb, r3status, r3len);
            break;
        }else
        {
            edit_buf_append(eb, " ", 1);
            len++;
        }
    }

    edit_buf_append(eb, "\033[m\r\n", 5);
}

void draw_message_bar( struct edit_buf* eb )
{
    edit_buf_append(eb, "\033[K", 3);

    int msglen = strlen(edit.statusmsg);
    if( msglen > edit.term_cols )
    {
        msglen = edit.term_cols;
    }

    if( msglen && time(NULL) - edit.statusmsg_time < 5 )
    {
        edit_buf_append(eb, edit.statusmsg, msglen);
    }else
    {
        edit_buf_append(eb, " ^Q - Quit | ^S - Save | ^F - Find", 34);
    }
}

void set_statusmsg( const char* fmt, ... )
{
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(edit.statusmsg, sizeof(edit.statusmsg), fmt, ap);
    va_end(ap);
    edit.statusmsg_time = time(NULL);
}

void scroll( void )
{
    edit.rx = 0;
    if( edit.cy < edit.numrows )
    {
        edit.rx = cx_to_rx(&edit.rows[edit.cy], edit.cx);
    }

    if( edit.cy < edit.scroll_rows )
    {
        edit.scroll_rows = edit.cy;
    }

    if( edit.cy >= edit.scroll_rows + edit.term_rows )
    {
        edit.scroll_rows = edit.cy - edit.term_rows + 1;
    }

    if( edit.rx < edit.scroll_cols )
    {
        edit.scroll_cols = edit.rx;
    }

    if( edit.rx >= edit.scroll_cols + edit.term_cols )
    {
        edit.scroll_cols = edit.rx - edit.term_cols + 1;
    }
}

void edit_buf_append( struct edit_buf* eb, const char* s, int len )
{
    char* new = realloc(eb->buf, eb->len + len);

    if( new == NULL )
    {
        return;
    }
    memcpy(&new[eb->len], s, len);
    eb->buf = new;
    eb->len += len;
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

    update_syntax(row);
}

void insert_row( int at, char* s, size_t len )
{
    if( at < 0 || at > edit.numrows )
    {
        return;
    }

    edit.rows = realloc(edit.rows, sizeof(edit_row) * (edit.numrows + 1));
    memmove(&edit.rows[at + 1], &edit.rows[at], sizeof(edit_row) * (edit.numrows - at));
    for( int j = at + 1; j <= edit.numrows; j++ )
    {
        edit.rows[j].idx++;
    }

    edit.rows[at].idx = at;
    edit.rows[at].size = len;
    edit.rows[at].chars = malloc(len + 1);
    memcpy(edit.rows[at].chars, s, len);
    edit.rows[at].chars[len] = '\0';

    edit.rows[at].rsize = 0;
    edit.rows[at].render = NULL;
    edit.rows[at].highlight = NULL;
    edit.rows[at].highlight_open_comment = 0;
    update_row(&edit.rows[at]);

    edit.numrows++;
    edit.dirty++;
}

void free_row( edit_row* row )
{
    free(row->render);
    free(row->chars);
    free(row->highlight);
}

void del_row( int at )
{
    if( at < 0 || at >= edit.numrows )
    {
        return;
    }
    free_row(&edit.rows[at]);
    memmove(&edit.rows[at], &edit.rows[at+1], sizeof(edit_row) * (edit.numrows - at - 1));
    for( int j = 0; j < edit.numrows - 1; j++ )
    {
        edit.rows[j].idx--;
    }
    edit.numrows--;
    edit.dirty++;
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
    edit.dirty++;
}

void row_append_string( edit_row* row, char* s, size_t len )
{
    row->chars = realloc(row->chars, row->size + len + 1);
    memcpy(&row->chars[row->size], s, len);
    row->size += len;
    row->chars[row->size] = '\0';
    update_row(row);

    edit.dirty++;
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

    edit.dirty++;
}

void insert_char( int c )
{
    if( edit.cy == edit.numrows )
    {
        insert_row(edit.numrows, "", 0);
    }

    row_insert_char(&edit.rows[edit.cy], edit.cx, c);
    edit.cx++;
}

void insert_newline()
{
    if( edit.cx == 0 )
    {
        insert_row(edit.cy, "", 0);
    }else
    {
        edit_row* row = &edit.rows[edit.cy];
        insert_row(edit.cy + 1, &row->chars[edit.cx], row->size - edit.cx);
        row = &edit.rows[edit.cy];
        row->size = edit.cx;
        row->chars[row->size] = '\0';
        update_row(row);
    }

    if( edit.numrows != 1 )
    {
        edit.cy++;
    }
    edit.cx = 0;
}

void del_char()
{
    if( edit.cy == edit.numrows )
    {
        return;
    }
    if( edit.cx == 0 && edit.cy == 0 )
    {
        return;
    }

    edit_row* row = &edit.rows[edit.cy];
    if( edit.cx > 0 )
    {
        row_del_char(row, edit.cx - 1);
        edit.cx--;
    }else
    {
        edit.cx = edit.rows[edit.cy - 1].size;
        row_append_string(&edit.rows[edit.cy - 1], row->chars, row->size);
        del_row(edit.cy);
        edit.cy--;
    }
}

void open_file( char* filename )
{
    free(edit.filename);
    edit.filename = strdup(filename);

    syntax_from_file_extension();
    
    FILE* fp = fopen(filename, "r");
    if( !fp )
    {
        edit.newfile = 1;
        return;
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
        insert_row(edit.numrows, line, linelen);
    }

    free(line);
    fclose(fp);

    edit.dirty = 0;
}

void save_file( void )
{
    if( edit.filename == NULL )
    {
        edit.filename = status_bar_prompt("Save as: %s", NULL);
        if( edit.filename == NULL )
        {
            set_statusmsg("Save cancelled");
            return;
        }
        syntax_from_file_extension();
    }

    int len;
    char* buf = rows_to_string(&len);

    int fd = open(edit.filename, O_RDWR | O_CREAT | O_TRUNC, 0644);
    if( fd != -1 )
    {
        write(fd, buf, len);
        close(fd);
        edit.newfile = 0;
        edit.dirty = 0;
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
    for( j = 0; j < edit.numrows; j++ )
    {
        len += edit.rows[j].size + 1;
    }
    *buflen = len;

    char* buf = malloc(len);
    char* p = buf;
    for( j = 0; j < edit.numrows; j++ )
    {
        memcpy(p, edit.rows[j].chars, edit.rows[j].size);
        p += edit.rows[j].size;
        *p = '\n';
        p++;
    }

    return buf;
}

void find( void )
{
    int save_cx = edit.cx;
    int save_cy = edit.cy;
    int save_scroll_rows = edit.scroll_rows;
    int save_scroll_cols = edit.scroll_cols;

    char* query = status_bar_prompt("Search: %s", find_callback);
    
    if( query )
    {
        free(query);
    }else
    {
        edit.cx = save_cx;
        edit.cy = save_cy;
        edit.scroll_rows = save_scroll_rows;
        edit.scroll_cols = save_scroll_cols;
    }
}

void find_callback( char* query, int key )
{
    static int last_match = -1;
    static int direction = 1;

    static int saved_highlight_line;
    static char* saved_highlight = NULL;

    if( saved_highlight )
    {
        memcpy(edit.rows[saved_highlight_line].highlight, saved_highlight, edit.rows[saved_highlight_line].rsize);
        free(saved_highlight);
        saved_highlight = NULL;
    }

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
    for( i = 0; i < edit.numrows; i++ )
    {
        current += direction;
        if( current == -1 )
        {
            current = edit.numrows - 1;
        }else if( current == edit.numrows )
        {
            current = 0;
        }

        edit_row* row = &edit.rows[current];
        char* match = strstr(row->render, query);
        if( match )
        {
            last_match = current;
            edit.cy = current;
            edit.cx = rx_to_cx(row, match - row->render);
            edit.scroll_rows = edit.numrows;

            saved_highlight_line = current;
            saved_highlight = malloc(row->rsize);
            memcpy(saved_highlight, row->highlight, row->rsize);
            memset(&row->highlight[match - row->render], HL_MATCH, strlen(query));
            break;
        }
    }
}

void update_syntax( edit_row* row )
{
    row->highlight = realloc(row->highlight, row->rsize);
    memset(row->highlight, HL_NORMAL, row->rsize);

    if( edit.syntax == NULL )
    {
        return;
    }

    char** keywords = edit.syntax->keywords;
    char** macros = edit.syntax->macros;

    char* scs = edit.syntax->single_line_comment_start;
    char* mcs = edit.syntax->multi_line_comment_start;
    char* mce = edit.syntax->multi_line_comment_end;

    int scs_len = scs? strlen(scs) : 0;
    int mcs_len = mcs? strlen(mcs) : 0;
    int mce_len = mce? strlen(mce) : 0;

    int prev_sep = 1;
    int in_string = 0;
    int in_comment = (row->idx > 0 && row_has_open_comment(&edit.rows[row->idx - 1]));

    int i = 0;
    char* p = row->render;

    while( *p && isspace(*p) )
    {
        p++;
        i++;
    }

    while( *p )
    {
        if( scs_len && !in_string && !in_comment )
        {
            if( prev_sep && !strncmp(p, scs, scs_len) )
            {
                memset(row->highlight + i, HL_COMMENT, row->rsize - i);
                return;
            }
        }

        if( mcs_len && mce_len && !in_string )
        {
            if( in_comment )
            {
                row->highlight[i] = HL_MLCOMMENT;
                if( !strncmp(p, mce, mce_len) )
                {
                    row->highlight[i + 1] = HL_MLCOMMENT;
                    p += mce_len;
                    i += mce_len;
                    in_comment = 0;
                    prev_sep = 1;
                    continue;
                }else
                {
                    prev_sep = 0;
                    p++;
                    i++;
                    continue;
                }
            }else if( !strncmp(p, mcs, mcs_len) )
            {
                memset(&row->highlight[i], HL_MLCOMMENT, mcs_len);
                p += mcs_len;
                i += mcs_len;
                in_comment = 1;
                prev_sep = 0;
                continue;
            }
        }

        if( edit.syntax->flags & HL_MACROS )
        {
            if( row->render[0] == '#' )
            {
                int j;
                for( j = 0; macros[j]; j++ )
                {
                    int mlen = strlen(macros[j]);

                    if( !strncmp(&row->render[i], macros[j], mlen) )
                    {
                        memset(&row->highlight[i], HL_MACRO, row->rsize - i);
                        return;
                    }
                }
            }
        }

        if( edit.syntax->flags & HL_STRINGS )
        {
            if( in_string )
            {
                row->highlight[i] = HL_STRING;
                if( *p == '\\' && i + 1 < row->rsize )
                {
                    row->highlight[i + 1] = HL_STRING;
                    p += 2;
                    i += 2;
                    prev_sep = 0;
                    continue;
                }
                if( *p == in_string )
                {
                    in_string = 0;
                }
                p++;
                i++;
                continue;
            }else
            {
                if( *p == '"' || *p == '\'' )
                {
                    in_string = *p;
                    row->highlight[i] = HL_STRING;
                    p++;
                    i++;
                    prev_sep = 0;
                    continue;
                }       
            }
        }

        if( edit.syntax->flags & HL_NUMBERS )
        {
            if( (isdigit(*p) && (prev_sep || row->highlight[i - 1] == HL_NUMBER)) || 
                (*p == '.' && row->highlight[i - 1] == HL_NUMBER) ||
                (*p == 'x' && row->highlight[i - 1] == HL_NUMBER))
            {
                row->highlight[i] = HL_NUMBER;
                p++;
                i++;
                prev_sep = 0;
                continue;
            }
        }

        if( prev_sep )
        {
            int j;
            for( j = 0; keywords[j]; j++ )
            {
                int klen = strlen(keywords[j]);
                int kw2 = keywords[j][klen - 1] == '|';
                if (kw2)
                {
                    klen--;
                }

                if( !strncmp(p, keywords[j], klen) && (klen < row->rsize - i) && is_seperator(*(p + klen)) )
                {
                    memset(&row->highlight[i], kw2 ? HL_KEYWORD2 : HL_KEYWORD1, klen);
                    p += klen;
                    i += klen;
                    break;
                }
            }
            if( keywords[j] != NULL )
            {
                prev_sep = 0;
                continue;
            }
        }

        prev_sep = is_seperator(*p);
        p++;
        i++;
    }

    int open_comment = row_has_open_comment(row);
    if( row->highlight_open_comment != open_comment && row->idx+1 < edit.numrows )
    {
        update_syntax(&edit.rows[row->idx+1]);
    }
    row->highlight_open_comment = open_comment;
}

int row_has_open_comment( edit_row* row )
{
    return( row->highlight && row->rsize && row->highlight[row->rsize-1] == HL_MLCOMMENT && 
            (row->rsize < 2 || (row->render[row->rsize-2] != '*' || row->render[row->rsize-1] != '/')) );
}

int syntax_to_color( int highlight )
{
    switch( highlight )
    {
        case HL_COMMENT:    return 93;
        case HL_MLCOMMENT:  return 93;
        case HL_KEYWORD1:   return 96;
        case HL_KEYWORD2:   return 36;
        case HL_MACRO:      return 92;
        case HL_STRING:     return 94;
        case HL_NUMBER:     return 33;
        case HL_MATCH:      return 41;
        default:            return 0;
    }
}

int is_seperator( int c )
{
    return isspace(c) || c == '\0' || strchr(",.()+-/*=~%<>[]{};", c) != NULL;
}

void syntax_from_file_extension( void )
{
    edit.syntax = NULL;
    if( edit.filename == NULL )
    {
        return;
    }

    char* extension = strrchr(edit.filename, '.');

    for( unsigned int j = 0; j < HLDB_ENTRIES; j++ )
    {
        struct syntax* s = &HLDB[j];
        unsigned int i = 0;
        while( s->filematch[i] )
        {
            int is_ext = (s->filematch[i][0] == '.');
            if( (is_ext && extension && !strcmp(extension, s->filematch[i]) ) ||
                (!is_ext && strstr(edit.filename, s->filematch[i])) )
            {
                edit.syntax = s;

                int file_row;
                for( file_row = 0; file_row < edit.numrows; file_row++ )
                {
                    update_syntax(&edit.rows[file_row]);
                }

                return;
            }
            i++;
        }
    }
}
