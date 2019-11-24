#include <drivers/keyboard.h>

#define DEBUG_SCANCODES 0

#define KEY_UP_MASK   0x80
#define KEY_CODE_MASK 0x7F
#define KEY_CTRL_MASK 0x40

#define norm 0x01
#define spec 0x02
#define func 0x03

#define SET_UNSET(a,b,c) (a) = (c) ? ((a) | (b)) : ((a) & ~(b))

extern void handle_input_char( char );
extern void handle_input_string( char* );

void key_event( int ret, key_event_t* event )
{
    if( ret )
    {
        /* ENTER = reads as linefeed, should be carriage return */
        if( event->keycode == 10 )
        {
            handle_input_char('\r');
            return;
        }

        /* BACKSPACE = reads as ^H, should be ^? */
        if( event->keycode == 8 )
        {
            handle_input_char(0x7F);
            return;
        }

        handle_input_char(event->key);
    }else
    {
        if( event->action == KEY_ACTION_UP )
        {
            return;
        }
        switch( event->keycode )
        {
            case KEY_F1:
                handle_input_string("\033OP");
                break;
            case KEY_F2:
                handle_input_string("\033OQ");
                break;
            case KEY_F3:
                handle_input_string("\033OR");
                break;
            case KEY_F4:
                handle_input_string("\033OS");
                break;
            case KEY_F5:
                handle_input_string("\033[15~");
                break;
            case KEY_F6:
                handle_input_string("\033[17~");
                break;
            case KEY_F7:
                handle_input_string("\033[18~");
                break;
            case KEY_F8:
                handle_input_string("\033[19~");
                break;
            case KEY_F9:
                handle_input_string("\033[20~");
                break;
            case KEY_F10:
                handle_input_string("\033[21~");
                break;
            case KEY_F11:
                handle_input_string("\033[23~");
                break;
            case KEY_F12:
                handle_input_string("\033[24~");
                break;
            case KEY_ARROW_UP:
                if( event->modifiers & KEY_MOD_LEFT_SHIFT && event->modifiers & KEY_MOD_LEFT_CTRL )
                {
                    handle_input_string("\033[6A");
                }else if( event->modifiers & KEY_MOD_LEFT_CTRL )
                {
                    handle_input_string("\033[5A");
                }else if( event->modifiers & KEY_MOD_LEFT_SHIFT && event->modifiers & KEY_MOD_LEFT_ALT )
                {
                    handle_input_string("\033[4A");
                }else if( event->modifiers & KEY_MOD_LEFT_ALT )
                {
                    handle_input_string("\033[3A");
                }else if( event->modifiers & KEY_MOD_LEFT_SHIFT )
                {
                    handle_input_string("\033[2A");
                }else
                {
                    handle_input_string("\033[A");
                }
                break;
            case KEY_ARROW_DOWN:
                if( event->modifiers & KEY_MOD_LEFT_SHIFT && event->modifiers & KEY_MOD_LEFT_CTRL )
                {
                    handle_input_string("\033[6B");
                }else if( event->modifiers & KEY_MOD_LEFT_CTRL )
                {
                    handle_input_string("\033[5B");
                }else if( event->modifiers & KEY_MOD_LEFT_SHIFT && event->modifiers & KEY_MOD_LEFT_ALT )
                {
                    handle_input_string("\033[4B");
                }else if( event->modifiers & KEY_MOD_LEFT_ALT )
                {
                    handle_input_string("\033[3B");
                }else if( event->modifiers & KEY_MOD_LEFT_SHIFT )
                {
                    handle_input_string("\033[2B");
                }else
                {
                    handle_input_string("\033[B");
                }
                break;
            case KEY_ARROW_RIGHT:
                if( event->modifiers & KEY_MOD_LEFT_SHIFT && event->modifiers & KEY_MOD_LEFT_CTRL )
                {
                    handle_input_string("\033[6C");
                }else if( event->modifiers & KEY_MOD_LEFT_CTRL )
                {
                    handle_input_string("\033[5C");
                }else if( event->modifiers & KEY_MOD_LEFT_SHIFT && event->modifiers & KEY_MOD_LEFT_ALT )
                {
                    handle_input_string("\033[4C");
                }else if( event->modifiers & KEY_MOD_LEFT_ALT )
                {
                    handle_input_string("\033[3C");
                }else if( event->modifiers & KEY_MOD_LEFT_SHIFT )
                {
                    handle_input_string("\033[2C");
                }else
                {
                    handle_input_string("\033[C");
                }
                break;
            case KEY_ARROW_LEFT:
                if( event->modifiers & KEY_MOD_LEFT_SHIFT && event->modifiers & KEY_MOD_LEFT_CTRL )
                {
                    handle_input_string("\033[6D");
                }else if( event->modifiers & KEY_MOD_LEFT_CTRL )
                {
                    handle_input_string("\033[5D");
                }else if( event->modifiers & KEY_MOD_LEFT_SHIFT && event->modifiers & KEY_MOD_LEFT_ALT )
                {
                    handle_input_string("\033[4D");
                }else if( event->modifiers & KEY_MOD_LEFT_ALT )
                {
                    handle_input_string("\033[3D");
                }else if( event->modifiers & KEY_MOD_LEFT_SHIFT )
                {
                    handle_input_string("\033[2D");
                }else
                {
                    handle_input_string("\033[D");
                }
                break;
            case KEY_PAGE_UP:
                handle_input_string("\033[5~");
                break;
            case KEY_PAGE_DOWN:
                handle_input_string("\033[6~");
                break;
            case KEY_HOME:
                handle_input_string("\033[H");
                break;
            case KEY_END:
                handle_input_string("\033[F");
                break;
            case KEY_DEL:
                handle_input_string("\033[3~");
                break;
        }
    }
}

char key_method[] = {
    /* 00 */ 0,    spec, norm, norm, norm, norm, norm, norm,
    /* 08 */ norm, norm, norm, norm, norm, norm, norm, norm,
    /* 10 */ norm, norm, norm, norm, norm, norm, norm, norm,
    /* 18 */ norm, norm, norm, norm, norm, spec, norm, norm,
    /* 20 */ norm, norm, norm, norm, norm, norm, norm, norm,
    /* 28 */ norm, norm, spec, norm, norm, norm, norm, norm,
    /* 30 */ norm, norm, norm, norm, norm, norm, spec, norm,
    /* 38 */ spec, norm, spec, func, func, func, func, func,
    /* 40 */ func, func, func, func, func, spec, spec, spec,
    /* 48 */ spec, spec, spec, spec, spec, spec, spec, spec,
    /* 50 */ spec, spec, spec, spec, spec, spec, spec, func,
    /* 58 */ func, spec, spec, spec, spec, spec, spec, spec,
    /* 60 */ spec, spec, spec, spec, spec, spec, spec, spec,
    /* 68 */ spec, spec, spec, spec, spec, spec, spec, spec,
    /* 70 */ spec, spec, spec, spec, spec, spec, spec, spec,
    /* 78 */ spec, spec, spec, spec, spec, spec, spec, spec,
};

char kbd_us[128] = {
    0, 27,
    '1','2','3','4','5','6','7','8','9','0',
    '-','=','\b',
    '\t', /* tab */
    'q','w','e','r','t','y','u','i','o','p','[',']','\n',
    0, /* control */
    'a','s','d','f','g','h','j','k','l',';','\'', '`',
    0, /* left shift */
    '\\','z','x','c','v','b','n','m',',','.','/',
    0, /* right shift */
    '*',
    0, /* alt */
    ' ', /* space */
    0, /* caps lock */
    0, /* F1 [59] */
    0, 0, 0, 0, 0, 0, 0, 0,
    0, /* ... F10 */
    0, /* 69 num lock */
    0, /* scroll lock */
    0, /* home */
    0, /* up */
    0, /* page up */
    '-',
    0, /* left arrow */
    0,
    0, /* right arrow */
    '+',
    0, /* 79 end */
    0, /* down */
    0, /* page down */
    0, /* insert */
    0, /* delete */
    0, 0, 0,
    0, /* F11 */
    0, /* F12 */
    0, /* everything else */
};

char kbd_us_caps[128] = {
    0, 27,
    '!','@','#','$','%','^','&','*','(',')',
    '_','+','\b',
    '\t', /* tab */
    'Q','W','E','R','T','Y','U','I','O','P','{','}','\n',
    0, /* control */
    'A','S','D','F','G','H','J','K','L',':','"', '~',
    0, /* left shift */
    '|','Z','X','C','V','B','N','M','<','>','?',
    0, /* right shift */
    '*',
    0, /* alt */
    ' ', /* space */
    0, /* caps lock */
    0, /* F1 [59] */
    0, 0, 0, 0, 0, 0, 0, 0,
    0, /* ... F10 */
    0, /* 69 num lock */
    0, /* scroll lock */
    0, /* home */
    0, /* up */
    0, /* page up */
    '-',
    0, /* left arrow */
    0,
    0, /* right arrow */
    '+',
    0, /* 79 end */
    0, /* down */
    0, /* page down */
    0, /* insert */
    0, /* delete */
    0, 0, 0,
    0, /* F11 */
    0, /* F12 */
    0, /* everything else */
};

/* Converts from incoming terminal keys to kbd_keys */
kbd_key_t kbd_key( key_event_state_t* state, unsigned char c )
{
    switch( state->kbd_state )
    {
        case KBD_NORMAL:
            switch( c )
            {
                case 0x1b:
                    state->kbd_state = KBD_ESC_A;
                    return KEY_NONE;
                default:
                    return c;
            }
        case KBD_ESC_A:
            switch( c )
            {
                case 0x5b:
                    state->kbd_state = KBD_ESC_B;
                    return KEY_NONE;
                case 'O':
                    state->kbd_state = KBD_ESC_O;
                    return KEY_NONE;
                default:
                    state->kbd_state = KBD_NORMAL;
                    return c;
            }
        case KBD_ESC_O:
            switch( c )
            {
                case 0x41:
                    state->kbd_state = KBD_NORMAL;
                    return KEY_ARROW_UP;
                case 0x42:
                    state->kbd_state = KBD_NORMAL;
                    return KEY_ARROW_DOWN;
                case 0x43:
                    state->kbd_state = KBD_NORMAL;
                    return KEY_ARROW_RIGHT;
                case 0x44:
                    state->kbd_state = KBD_NORMAL;
                    return KEY_ARROW_LEFT;
                case 'H':
                    state->kbd_state = KBD_NORMAL;
                    return KEY_HOME;
                case 'F':
                    state->kbd_state = KBD_NORMAL;
                    return KEY_END;
                default:
                    state->kbd_state = KBD_NORMAL;
                    return c;
            }
        case KBD_ESC_B:
            switch( c )
            {
                case 0x41:
                    state->kbd_state = KBD_NORMAL;
                    return KEY_ARROW_UP;
                case 0x42:
                    state->kbd_state = KBD_NORMAL;
                    return KEY_ARROW_DOWN;
                case 0x43:
                    state->kbd_state = KBD_NORMAL;
                    return KEY_ARROW_RIGHT;
                case 0x44:
                    state->kbd_state = KBD_NORMAL;
                    return KEY_ARROW_LEFT;
                case 'H':
                    state->kbd_state = KBD_NORMAL;
                    return KEY_HOME;
                case 'F':
                    state->kbd_state = KBD_NORMAL;
                    return KEY_END;
                case '1':
                case '2':
                case '3':
                case '4':
                case '5':
                case '6':
                    state->kbd_esc_buf = c;
                    state->kbd_state = KBD_ESC_EXT;
                    return KEY_NONE;
                default:
                    state->kbd_state = KBD_NORMAL;
                    return c;
            }
        case KBD_ESC_EXT:
            switch( c )
            {
                case '~':
                    switch( state->kbd_esc_buf )
                    {
                        case '2':
                            state->kbd_state = KBD_NORMAL;
                            return KEY_INSERT;
                        case '3':
                            state->kbd_state = KBD_NORMAL;
                            return KEY_DEL;
                        case '5':
                            state->kbd_state = KBD_NORMAL;
                            return KEY_PAGE_UP;
                        case '6':
                            state->kbd_state = KBD_NORMAL;
                            return KEY_PAGE_DOWN;
                        default:
                            return c;
                    }
                case 'A':
                    switch( state->kbd_esc_buf )
                    {
                        case '2':
                            state->kbd_state = KBD_NORMAL;
                            return KEY_SHIFT_ARROW_UP;
                        case '5':
                            state->kbd_state = KBD_NORMAL;
                            return KEY_CTRL_ARROW_UP;
                        /* Other modifier states? */
                        default:
                            return c;
                    }
                case 'B':
                    switch( state->kbd_esc_buf )
                    {
                        case '2':
                            state->kbd_state = KBD_NORMAL;
                            return KEY_SHIFT_ARROW_DOWN;
                        case '5':
                            state->kbd_state = KBD_NORMAL;
                            return KEY_CTRL_ARROW_DOWN;
                        /* Other modifier states? */
                        default:
                            return c;
                    }
                case 'C':
                    switch( state->kbd_esc_buf )
                    {
                        case '2':
                            state->kbd_state = KBD_NORMAL;
                            return KEY_SHIFT_ARROW_RIGHT;
                        case '5':
                            state->kbd_state = KBD_NORMAL;
                            return KEY_CTRL_ARROW_RIGHT;
                        /* Other modifier states? */
                        default:
                            return c;
                    }
                case 'D':
                    switch( state->kbd_esc_buf )
                    {
                        case '2':
                            state->kbd_state = KBD_NORMAL;
                            return KEY_SHIFT_ARROW_LEFT;
                        case '5':
                            state->kbd_state = KBD_NORMAL;
                            return KEY_CTRL_ARROW_LEFT;
                        /* Other modifier states? */
                        default:
                            return c;
                    }
                case ';':
                    /* We don't produce multiple-argument escapes ourself, but
                     * we should handle them anyway because other terminals
                     * definitely do make them... */
                    state->kbd_state = KBD_ESC_B;
                    return KEY_NONE;
                default:
                    return c;
            }

        default:
            return KEY_BAD_STATE;
    }

    return KEY_BAD_STATE;
}

/* Convert scancodes to a series of keys */
int kbd_scancode(key_event_state_t * state, unsigned char c, key_event_t * event )
{
    event->keycode   = 0;
    event->action    = 0;
    event->modifiers = 0;
    event->key       = 0;

    event->modifiers |= state->kl_ctrl  ? KEY_MOD_LEFT_CTRL   : 0;
    event->modifiers |= state->kl_shift ? KEY_MOD_LEFT_SHIFT  : 0;
    event->modifiers |= state->kl_alt   ? KEY_MOD_LEFT_ALT    : 0;
    event->modifiers |= state->kl_super ? KEY_MOD_LEFT_SUPER  : 0;

    event->modifiers |= state->kr_ctrl  ? KEY_MOD_RIGHT_CTRL  : 0;
    event->modifiers |= state->kr_shift ? KEY_MOD_RIGHT_SHIFT : 0;
    event->modifiers |= state->kr_alt   ? KEY_MOD_RIGHT_ALT   : 0;
    event->modifiers |= state->kr_super ? KEY_MOD_RIGHT_SUPER : 0;

    if( !state->kbd_s_state )
    {
        if( c == 0xE0 )
        {
            state->kbd_s_state = 1;
            /* Literally nothing */
            return 0;
        }

        if( c & KEY_UP_MASK )
        {
            c ^= KEY_UP_MASK;
            event->action = KEY_ACTION_UP;
        }else
        {
            event->action = KEY_ACTION_DOWN;
        }
        int down = (event->action == KEY_ACTION_DOWN);

        switch( key_method[c] )
        {
            case norm:
                {
                    event->keycode = kbd_us[c];
                    if( state->k_ctrl )
                    {
                        int s = kbd_us[c];
                        if( s >= 'a' && s <= 'z') s -= 'a' - 'A';
                        if( s == '-') s = '_';
                        if( s == '`') s = '@';
                        int out = (int)(s - KEY_CTRL_MASK);
                        if( out < 0 || out > 0x1F )
                        {
                            event->key = kbd_us[c];
                        }else
                        {
                            event->key = (unsigned char)out;
                        }
                    }else
                    {
                        event->key = state->k_shift ? kbd_us_caps[c] : kbd_us[c];
                    }
                }
                break;
            case spec:
                switch( c )
                {
                    case 0x01:
                        event->key     = '\033';
                        event->keycode = KEY_ESCAPE;
                        break;
                    case 0x1D:
                        state->k_ctrl   = down;
                        state->kl_ctrl  = down;
                        event->keycode  = KEY_LEFT_CTRL;
                        SET_UNSET(event->modifiers, KEY_MOD_LEFT_CTRL, down);
                        break;
                    case 0x2A:
                        state->k_shift  = down;
                        state->kl_shift = down;
                        event->keycode  = KEY_LEFT_SHIFT;
                        SET_UNSET(event->modifiers, KEY_MOD_LEFT_SHIFT, down);
                        break;
                    case 0x36:
                        state->k_shift  = down;
                        state->kr_shift = down;
                        event->keycode  = KEY_RIGHT_SHIFT;
                        SET_UNSET(event->modifiers, KEY_MOD_RIGHT_SHIFT, down);
                        break;
                    case 0x38:
                        state->k_alt    = down;
                        state->kl_alt   = down;
                        event->keycode  = KEY_LEFT_ALT;
                        SET_UNSET(event->modifiers, KEY_MOD_LEFT_ALT, down);
                        break;
                    case KEY_SCANCODE_NUM_0:
                        event->keycode = KEY_NUM_0;
                        event->key = '0';
                        break;
                    case KEY_SCANCODE_NUM_1:
                        event->keycode = KEY_NUM_1;
                        event->key = '1';
                        break;
                    case KEY_SCANCODE_NUM_2:
                        event->keycode = KEY_NUM_2;
                        event->key = '2';
                        break;
                    case KEY_SCANCODE_NUM_3:
                        event->keycode = KEY_NUM_3;
                        event->key = '3';
                        break;
                    case KEY_SCANCODE_NUM_4:
                        event->keycode = KEY_NUM_4;
                        event->key = '4';
                        break;
                    case KEY_SCANCODE_NUM_5:
                        event->keycode = KEY_NUM_5;
                        event->key = '5';
                        break;
                    case KEY_SCANCODE_NUM_6:
                        event->keycode = KEY_NUM_6;
                        event->key = '6';
                        break;
                    case KEY_SCANCODE_NUM_7:
                        event->keycode = KEY_NUM_7;
                        event->key = '7';
                        break;
                    case KEY_SCANCODE_NUM_8:
                        event->keycode = KEY_NUM_8;
                        event->key = '8';
                        break;
                    case KEY_SCANCODE_NUM_9:
                        event->keycode = KEY_NUM_9;
                        event->key = '9';
                        break;
                    case KEY_SCANCODE_NUM_DOT:
                        event->keycode = KEY_NUM_DOT;
                        event->key = '.';
                        break;
                    case KEY_SCANCODE_NUM_MIN:
                        event->keycode = KEY_NUM_MINUS;
                        event->key = '-';
                        break;
                    case KEY_SCANCODE_NUM_ADD:
                        event->keycode = KEY_NUM_PLUS;
                        event->key = '+';
                        break;
                    default:
                        break;
                }
                break;
            case func:
                switch( c )
                {
                    case KEY_SCANCODE_F1:
                        event->keycode = KEY_F1;
                        break;
                    case KEY_SCANCODE_F2:
                        event->keycode = KEY_F2;
                        break;
                    case KEY_SCANCODE_F3:
                        event->keycode = KEY_F3;
                        break;
                    case KEY_SCANCODE_F4:
                        event->keycode = KEY_F4;
                        break;
                    case KEY_SCANCODE_F5:
                        event->keycode = KEY_F5;
                        break;
                    case KEY_SCANCODE_F6:
                        event->keycode = KEY_F6;
                        break;
                    case KEY_SCANCODE_F7:
                        event->keycode = KEY_F7;
                        break;
                    case KEY_SCANCODE_F8:
                        event->keycode = KEY_F8;
                        break;
                    case KEY_SCANCODE_F9:
                        event->keycode = KEY_F9;
                        break;
                    case KEY_SCANCODE_F10:
                        event->keycode = KEY_F10;
                        break;
                    case KEY_SCANCODE_F11:
                        event->keycode = KEY_F11;
                        break;
                    case KEY_SCANCODE_F12:
                        event->keycode = KEY_F12;
                        break;
                }
                break;
            default:
                break;
        }

        if( event->key )
        {
            return down;
        }

        return 0;
    }else if( state->kbd_s_state == 1 )
    {

        if( c & KEY_UP_MASK )
        {
            c ^= KEY_UP_MASK;
            event->action = KEY_ACTION_UP;
        }else
        {
            event->action = KEY_ACTION_DOWN;
        }

        int down = (event->action == KEY_ACTION_DOWN);
        switch( c )
        {
            case 0x5B:
                state->k_super  = down;
                state->kl_super = down;
                event->keycode  = KEY_LEFT_SUPER;
                SET_UNSET(event->modifiers, KEY_MOD_LEFT_SUPER, down);
                break;
            case 0x5C:
                state->k_super  = down;
                state->kr_super = down;
                event->keycode  = KEY_RIGHT_SUPER;
                SET_UNSET(event->modifiers, KEY_MOD_RIGHT_SUPER, down);
                break;
            case 0x1D:
                state->kr_ctrl  = down;
                state->k_ctrl   = down;
                event->keycode  = KEY_RIGHT_CTRL;
                SET_UNSET(event->modifiers, KEY_MOD_RIGHT_CTRL, down);
                break;
            case 0x38:
                state->kr_alt   = down;
                state->k_alt    = down;
                event->keycode  = KEY_RIGHT_ALT;
                SET_UNSET(event->modifiers, KEY_MOD_RIGHT_ALT, down);
                break;
            case 0x48:
                event->keycode = KEY_ARROW_UP;
                break;
            case 0x4D:
                event->keycode = KEY_ARROW_RIGHT;
                break;
            case 0x47:
                event->keycode = KEY_HOME;
                break;
            case 0x49:
                event->keycode = KEY_PAGE_UP;
                break;
            case 0x4B:
                event->keycode = KEY_ARROW_LEFT;
                break;
            case 0x4F:
                event->keycode = KEY_END;
                break;
            case 0x50:
                event->keycode = KEY_ARROW_DOWN;
                break;
            case 0x51:
                event->keycode = KEY_PAGE_DOWN;
                break;
            case 0x52:
                event->keycode = KEY_INSERT;
                break;
            case 0x53:
                event->keycode = KEY_DEL;
                break;
            case 0x35:
                event->keycode = KEY_NUM_DIV;
                event->key = '/';
                break;
            case 0x1C:
                event->keycode = KEY_NUM_ENTER;
                event->key = '\n';
                break;
            default:
                break;
        }

        state->kbd_s_state = 0;
        return 0;
    }
    return 0;
}
