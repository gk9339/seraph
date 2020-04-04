#include <libansiterm/libansiterm.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <spinlock.h>

static wchar_t box_chars[] = L"▒␉␌␍␊°±␤␋┘┐┌└┼⎺⎻─⎼⎽├┤┴┬│≤≥";

inline uint32_t rgb( uint8_t r, uint8_t g, uint8_t b )
{
    return 0xFF000000 | (r << 16) | (g << 8) | (b);
}

inline uint32_t rgba( uint8_t r, uint8_t g, uint8_t b, uint8_t a )
{
    return (a << 24U) | (r << 16) | (g << 8) | (b);
}

// Return the lower of two uint16_t
static uint16_t min( uint16_t a, uint16_t b )
{
    return (a < b) ? a : b;
}

// Return the lower of two uint16_t
static uint16_t max( uint16_t a, uint16_t b )
{
    return (a > b) ? a : b;
}

// Write the contents of the buffer, as they were all non-escaped data
static void ansi_dump_buffer( term_state_t* state )
{
    for( int i = 0; i < state->buflen; i++ )
    {
        state->callbacks->writer(state->buffer[i]);
    }
}

// Add to the internal buffer for the ANSI parser
static void ansi_buf_add( term_state_t* state, char c )
{
    if (state->buflen >= TERM_BUF_LEN-1)
    {
        return;
    }
    state->buffer[state->buflen] = c;
    state->buflen++;
    state->buffer[state->buflen] = '\0';
}

static int to_eight( uint32_t codepoint, char* out )
{
    memset(out, 0x00, 7);

    if (codepoint < 0x0080)
    {
        out[0] = (char)codepoint;
    }else if (codepoint < 0x0800)
    {
        out[0] = 0xC0 | (codepoint >> 6);
        out[1] = 0x80 | (codepoint & 0x3F);
    }else if (codepoint < 0x10000)
    {
        out[0] = 0xE0 | (codepoint >> 12);
        out[1] = 0x80 | ((codepoint >> 6) & 0x3F);
        out[2] = 0x80 | (codepoint & 0x3F);
    }else if (codepoint < 0x200000)
    {
        out[0] = 0xF0 | (codepoint >> 18);
        out[1] = 0x80 | ((codepoint >> 12) & 0x3F);
        out[2] = 0x80 | ((codepoint >> 6) & 0x3F);
        out[3] = 0x80 | ((codepoint) & 0x3F);
    }else if (codepoint < 0x4000000)
    {
        out[0] = 0xF8 | (codepoint >> 24);
        out[1] = 0x80 | (codepoint >> 18);
        out[2] = 0x80 | ((codepoint >> 12) & 0x3F);
        out[3] = 0x80 | ((codepoint >> 6) & 0x3F);
        out[4] = 0x80 | ((codepoint) & 0x3F);
    }else
    {
        out[0] = 0xF8 | (codepoint >> 30);
        out[1] = 0x80 | ((codepoint >> 24) & 0x3F);
        out[2] = 0x80 | ((codepoint >> 18) & 0x3F);
        out[3] = 0x80 | ((codepoint >> 12) & 0x3F);
        out[4] = 0x80 | ((codepoint >> 6) & 0x3F);
        out[5] = 0x80 | ((codepoint) & 0x3F);
    }

    return strlen(out);
}

// Actual ansi_put
static void _ansi_put( term_state_t* state, char c )
{
    term_callbacks_t * callbacks = state->callbacks;
    switch( state->escape )
        {
        case 0:
            // We are not escaped, check for escape character
            if (c == ANSI_ESCAPE )
            {
                // Enable escape mode, setup a buffer, fill the buffer, get out of here.
                state->escape    = 1;
                state->buflen    = 0;
                ansi_buf_add(state, c);
                return;
            }else if (c == 0 )
            {
                return;
            }else
            {
                if (state->box && c >= 'a' && c <= 'z' )
                {
                    char buf[7];
                    char *w = (char *)&buf;
                    to_eight(box_chars[c-'a'], w);
                    while (*w )
                    {
                        callbacks->writer(*w);
                        w++;
                    }
                }else
                {
                    callbacks->writer(c);
                }
            }
            break;
        case 1:
            // We're ready for [
            if (c == ANSI_BRACKET )
            {
                state->escape = 2;
                ansi_buf_add(state, c);
            }else if (c == ANSI_BRACKET_RIGHT )
            {
                state->escape = 3;
                ansi_buf_add(state, c);
            }else if (c == ANSI_OPEN_PAREN )
            {
                state->escape = 4;
                ansi_buf_add(state, c);
            }else if (c == 'T' )
            {
                state->escape = 5;
                ansi_buf_add(state, c);
            }else if (c == '7' )
            {
                state->escape = 0;
                state->buflen = 0;
                state->save_x = callbacks->get_csr_x();
                state->save_y = callbacks->get_csr_y();
            }else if (c == '8' )
            {
                state->escape = 0;
                state->buflen = 0;
                callbacks->set_csr(state->save_x, state->save_y);
            }else
            {
                // This isn't a bracket, we're not actually escaped, Get out of here!
                ansi_dump_buffer(state);
                callbacks->writer(c);
                state->escape = 0;
                state->buflen = 0;
                return;
            }
            break;
        case 2:
            if (c >= ANSI_LOW && c <= ANSI_HIGH )
            {
                char * pch;  // tokenizer pointer
                char * save; // strtok_r pointer
                char * argv[MAX_ARGS]; // escape arguments
                // Get rid of the front of the buffer
                strtok_r(state->buffer,"[",&save);
                pch = strtok_r(NULL,";",&save);
                int argc = 0;
                while (pch != NULL )
                {
                    argv[argc] = (char *)pch;
                    ++argc;
                    if (argc > MAX_ARGS)
                        break;
                    pch = strtok_r(NULL,";",&save);
                }
                switch( c )
                {
                    case ANSI_EXT_IOCTL:
                        {
                            if( argc > 0 )
                            {
                                int arg = atoi(argv[0]);
                                switch( arg )
                                {
                                    case 1:
                                        callbacks->draw_cursor();
                                        break;
                                    default:
                                        break;
                                }
                            }
                        }
                        break;
                    case ANSI_SCP:
                        {
                            state->save_x = callbacks->get_csr_x();
                            state->save_y = callbacks->get_csr_y();
                        }
                        break;
                    case ANSI_RCP:
                        {
                            callbacks->set_csr(state->save_x, state->save_y);
                        }
                        break;
                    case ANSI_SGR:
                        // Set Graphics Rendition
                        if (argc == 0 )
                        {
                            argv[0] = "0";
                            argc    = 1;
                        }
                        for (int i = 0; i < argc; ++i )
                        {
                            int arg = atoi(argv[i]);
                            if (arg >= 100 && arg < 110 )
                            {
                                // Bright background
                                state->bg = 8 + (arg - 100);
                                state->flags |= ANSI_SPECBG;
                            }else if (arg >= 90 && arg < 100 )
                            {
                                // Bright foreground
                                state->fg = 8 + (arg - 90);
                            }else if (arg >= 40 && arg < 49 )
                            {
                                // Set background
                                state->bg = arg - 40;
                                state->flags |= ANSI_SPECBG;
                            }else if (arg == 49 )
                            {
                                state->bg = TERM_DEFAULT_BG;
                                state->flags &= ~ANSI_SPECBG;
                            }else if (arg >= 30 && arg < 39 )
                            {
                                // Set Foreground
                                state->fg = arg - 30;
                            }else if (arg == 39 )
                            {
                                // Default Foreground
                                state->fg = 7;
                            }else if (arg == 24 )
                            {
                                // Underline off
                                state->flags &= ~ANSI_UNDERLINE;
                            }else if (arg == 23 )
                            {
                                // Oblique off
                                state->flags &= ~ANSI_ITALIC;
                            }else if (arg == 21 || arg == 22 )
                            {
                                // Bold off
                                state->flags &= ~ANSI_BOLD;
                            }else if (arg == 9 )
                            {
                                // X-OUT
                                state->flags |= ANSI_CROSS;
                            }else if (arg == 7 )
                            {
                                // INVERT: Swap foreground / background
                                uint32_t temp = state->fg;
                                state->fg = state->bg;
                                state->bg = temp;
                            }else if (arg == 6 )
                            {
                                // proprietary RGBA color support
                                if (i == 0 )
                                {
                                    break;
                                }
                                if (i < argc )
                                {
                                    int r = atoi(argv[i+1]);
                                    int g = atoi(argv[i+2]);
                                    int b = atoi(argv[i+3]);
                                    int a = atoi(argv[i+4]);
                                    if (a == 0) a = 1; // Override a = 0
                                    uint32_t c = rgba(r,g,b,a);
                                    if (atoi(argv[i-1]) == 48 )
                                    {
                                        state->bg = c;
                                        state->flags |= ANSI_SPECBG;
                                    }else if (atoi(argv[i-1]) == 38 )
                                    {
                                        state->fg = c;
                                    }
                                    i += 4;
                                }
                            }else if (arg == 5 )
                            {
                                // Supposed to be blink; instead, support X-term 256 colors
                                if (i == 0 )
                                {
                                    break;
                                }
                                if (i < argc )
                                {
                                    if (atoi(argv[i-1]) == 48 )
                                    {
                                        // Background to i+1
                                        state->bg = atoi(argv[i+1]);
                                        state->flags |= ANSI_SPECBG;
                                    }else if (atoi(argv[i-1]) == 38 )
                                    {
                                        // Foreground to i+1
                                        state->fg = atoi(argv[i+1]);
                                    }
                                    ++i;
                                }
                            }else if (arg == 4 )
                            {
                                // UNDERLINE
                                state->flags |= ANSI_UNDERLINE;
                            }else if (arg == 3 )
                            {
                                // ITALIC: Oblique
                                state->flags |= ANSI_ITALIC;
                            }else if (arg == 2 )
                            {
                                // Konsole RGB color support
                                if (i == 0 )
                                {
                                    break; 
                                }
                                if (i < argc - 2 )
                                {
                                    int r = atoi(argv[i+1]);
                                    int g = atoi(argv[i+2]);
                                    int b = atoi(argv[i+3]);
                                    uint32_t c = rgb(r,g,b);
                                    if (atoi(argv[i-1]) == 48 )
                                    {
                                        // Background to i+1
                                        state->bg = c;
                                        state->flags |= ANSI_SPECBG;
                                    }else if (atoi(argv[i-1]) == 38 )
                                    {
                                        // Foreground to i+1
                                        state->fg = c;
                                    }
                                    i += 3;
                                }
                            }else if (arg == 1 )
                            {
                                // BOLD/BRIGHT: Brighten the output color
                                state->flags |= ANSI_BOLD;
                            }else if (arg == 0 )
                            {
                                // Reset everything
                                state->fg = TERM_DEFAULT_FG;
                                state->bg = TERM_DEFAULT_BG;
                                state->flags = TERM_DEFAULT_FLAGS;
                            }
                        }
                        break;
                    case ANSI_SHOW:
                        if (argc > 0 )
                        {
                            if (!strcmp(argv[0], "?1049") )
                            {
                                if (callbacks->switch_buffer) callbacks->switch_buffer(1);
                            }else if (!strcmp(argv[0], "?1000") )
                            {
                                state->mouse_status |= ANSITERM_MOUSE_ENABLE;
                            }else if (!strcmp(argv[0], "?1002") )
                            {
                                state->mouse_status |= ANSITERM_MOUSE_DRAG;
                            }else if (!strcmp(argv[0], "?1006") )
                            {
                                state->mouse_status |= ANSITERM_MOUSE_SGR;
                            }else if (!strcmp(argv[0], "?25") )
                            {
                                callbacks->enable_csr(1);
                            }else if (!strcmp(argv[0], "?2004") )
                            {
                                state->paste_mode = 1;
                            }
                        }
                        break;
                    case ANSI_HIDE:
                        if (argc > 0 )
                        {
                            if (!strcmp(argv[0], "?1049") )
                            {
                                if (callbacks->switch_buffer) callbacks->switch_buffer(0);
                            }else if (!strcmp(argv[0], "?1000") )
                            {
                                state->mouse_status &= ~ANSITERM_MOUSE_ENABLE;
                            }else if (!strcmp(argv[0], "?1002") )
                            {
                                state->mouse_status &= ~ANSITERM_MOUSE_DRAG;
                            }else if (!strcmp(argv[0],"?1006") )
                            {
                                state->mouse_status &= ~ANSITERM_MOUSE_SGR;
                            }else if (!strcmp(argv[0], "?25") )
                            {
                                callbacks->enable_csr(0);
                            }else if (!strcmp(argv[0], "?2004") )
                            {
                                state->paste_mode = 0;
                            }
                        }
                        break;
                    case ANSI_CUF:
                        {
                            int i = 1;
                            if (argc )
                            {
                                i = atoi(argv[0]);
                            }
                            callbacks->set_csr(min(callbacks->get_csr_x() + i, state->width - 1), callbacks->get_csr_y());
                        }
                        break;
                    case ANSI_CUU:
                        {
                            int i = 1;
                            if (argc )
                            {
                                i = atoi(argv[0]);
                            }
                            callbacks->set_csr(callbacks->get_csr_x(), max(callbacks->get_csr_y() - i, 0));
                        }
                        break;
                    case ANSI_CUD:
                        {
                            int i = 1;
                            if (argc )
                            {
                                i = atoi(argv[0]);
                            }
                            callbacks->set_csr(callbacks->get_csr_x(), min(callbacks->get_csr_y() + i, state->height - 1));
                        }
                        break;
                    case ANSI_CUB:
                        {
                            int i = 1;
                            if (argc )
                            {
                                i = atoi(argv[0]);
                            }
                            callbacks->set_csr(max(callbacks->get_csr_x() - i,0), callbacks->get_csr_y());
                        }
                        break;
                    case ANSI_CHA:
                        if (argc < 1 )
                        {
                            callbacks->set_csr(0,callbacks->get_csr_y());
                        }else
                        {
                            callbacks->set_csr(min(max(atoi(argv[0]), 1), state->width) - 1, callbacks->get_csr_y());
                        }
                        break;
                    case ANSI_CUP:
                        if (argc < 2 )
                        {
                            callbacks->set_csr(0,0);
                        }else
                        {
                            callbacks->set_csr(min(max(atoi(argv[1]), 1), state->width) - 1, min(max(atoi(argv[0]), 1), state->height) - 1);
                        }
                        break;
                    case ANSI_ED:
                        if (argc < 1 )
                        {
                            callbacks->cls(0);
                        }else
                        {
                            callbacks->cls(atoi(argv[0]));
                        }
                        break;
                    case ANSI_EL:
                        {
                            int what = 0, x = 0, y = 0;
                            if (argc >= 1 )
                            {
                                what = atoi(argv[0]);
                            }
                            if (what == 0 )
                            {
                                x = callbacks->get_csr_x();
                                y = state->width;
                            }else if (what == 1 )
                            {
                                x = 0;
                                y = callbacks->get_csr_x();
                            }else if (what == 2 )
                            {
                                x = 0;
                                y = state->width;
                            }
                            for (int i = x; i < y; ++i )
                            {
                                callbacks->set_cell(i, callbacks->get_csr_y(), ' ');
                            }
                        }
                        break;
                    case ANSI_DSR:
                        {
                            char out[24];
                            sprintf(out, "\033[%d;%dR", callbacks->get_csr_y() + 1, callbacks->get_csr_x() + 1);
                            callbacks->handle_input(out);
                        }
                        break;
                    case ANSI_SU:
                        {
                            int how_many = 1;
                            if (argc > 0 )
                            {
                                how_many = atoi(argv[0]);
                            }
                            callbacks->scroll(how_many);
                        }
                        break;
                    case ANSI_SD:
                        {
                            int how_many = 1;
                            if (argc > 0 )
                            {
                                how_many = atoi(argv[0]);
                            }
                            callbacks->scroll(-how_many);
                        }
                        break;
                    case ANSI_IL:
                        {
                            int how_many = 1;
                            if (argc > 0 )
                            {
                                how_many = atoi(argv[0]);
                            }
                            callbacks->insert_delete_lines(how_many);
                        }
                        break;
                    case ANSI_DL:
                        {
                            int how_many = 1;
                            if (argc > 0 )
                            {
                                how_many = atoi(argv[0]);
                            }
                            callbacks->insert_delete_lines(-how_many);
                        }
                        break;
                    case 'X':
                        {
                            int how_many = 1;
                            if (argc > 0 )
                            {
                                how_many = atoi(argv[0]);
                            }
                            for (int i = 0; i < how_many; ++i )
                            {
                                callbacks->writer(' ');
                            }
                        }
                        break;
                    case 'd':
                        if (argc < 1 )
                        {
                            callbacks->set_csr(callbacks->get_csr_x(), 0);
                        }else
                        {
                            callbacks->set_csr(callbacks->get_csr_x(), atoi(argv[0]) - 1);
                        }
                        break;
                    default:
                        // Meh
                        break;
                }
                // Set the states
                if (state->flags & ANSI_BOLD && state->fg < 9 )
                {
                    callbacks->set_color(state->fg % 8 + 8, state->bg);
                }else
                {
                    callbacks->set_color(state->fg, state->bg);
                }
                // Clear out the buffer
                state->buflen = 0;
                state->escape = 0;
                return;
            }else
            {
                // Still escaped
                ansi_buf_add(state, c);
            }
            break;
        case 3:
            if (c == '\007' )
            {
                // Tokenize on semicolons, like we always do
                char * pch;  // tokenizer pointer
                char * save; // strtok_r pointer
                char * argv[MAX_ARGS]; // escape arguments
                // Get rid of the front of the buffer
                strtok_r(state->buffer,"]",&save);
                pch = strtok_r(NULL,";",&save);
                // argc = Number of arguments, obviously
                int argc = 0;
                while (pch != NULL )
                {
                    argv[argc] = (char *)pch;
                    ++argc;
                    if (argc > MAX_ARGS) break;
                    pch = strtok_r(NULL,";",&save);
                }
                // Start testing the first argument for what command to use
                if (argv[0] )
                {
                    if (!strcmp(argv[0], "1") )
                    {
                        if (argc > 1 )
                        {
                            callbacks->set_title(argv[1]);
                        }
                    } // Currently, no other options
                }
                // Clear out the buffer
                state->buflen = 0;
                state->escape = 0;
                return;
            }else
            {
                // Still escaped
                if (c == '\n' || state->buflen == 255 )
            {
                    ansi_dump_buffer(state);
                    callbacks->writer(c);
                    state->buflen = 0;
                    state->escape = 0;
                    return;
                }
                ansi_buf_add(state, c);
            }
            break;
        case 4:
            if (c == '0' )
            {
                state->box = 1;
            }else if (c == 'B' )
            {
                state->box = 0;
            }else
            {
                ansi_dump_buffer(state);
                callbacks->writer(c);
            }
            state->escape = 0;
            state->buflen = 0;
            break;
        case 5:
            if (c == 'q' )
            {
                char out[24];
                sprintf(out, "\033T%d;%dq", callbacks->get_cell_width(), callbacks->get_cell_height());
                callbacks->handle_input(out);
                state->escape = 0;
                state->buflen = 0;
            }else if (c == 's' )
            {
                state->img_collected = 0;
                state->escape = 6;
                state->img_size = sizeof(uint32_t) * callbacks->get_cell_width() * callbacks->get_cell_height();
                if (!state->img_data )
                {
                    state->img_data = malloc(state->img_size);
                }
                memset(state->img_data, 0x00, state->img_size);
            }else
            {
                ansi_dump_buffer(state);
                callbacks->writer(c);
                state->escape = 0;
                state->buflen = 0;
            }
            break;
        case 6:
            state->img_data[state->img_collected++] = c;
            if (state->img_collected == state->img_size )
            {
                callbacks->set_cell_contents(callbacks->get_csr_x(), callbacks->get_csr_y(), state->img_data);
                callbacks->set_csr(min(callbacks->get_csr_x() + 1, state->width - 1), callbacks->get_csr_y());
                state->escape = 0;
                state->buflen = 0;
            }
            break;
    }
}

void ansi_put( term_state_t* state, char c )
{
    spin_lock(&state->lock);
    _ansi_put(state, c);
    spin_unlock(&state->lock);
}

term_state_t* ansi_init( term_state_t* state, int width, int height, term_callbacks_t* callbacks )
{
    if( !state )
    {
        state = malloc(sizeof(term_state_t));
    }
    memset(state, 0, sizeof(term_state_t));

    state->fg = TERM_DEFAULT_FG;
    state->bg = TERM_DEFAULT_BG;
    state->flags = TERM_DEFAULT_FLAGS;
    state->width = width;
    state->height = height;
    state->box = 0;
    state->callbacks = callbacks;
    state->callbacks->set_color(state->fg, state->bg);
    state->mouse_status = 0;

    return state;
}
