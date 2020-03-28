#include <stdio.h>
#include <stdint.h>
#include <stdarg.h>
#include <string.h>
#include <stdbool.h>
#include <ctype.h>

static char* __int_str(intmax_t i, char b[], int base, bool plus_sign, bool space_no_sign, int padding_no, bool justify, bool zero_pad)
{
    char digit[32] = {0};
    memset(digit, 0, 32);
    strcpy(digit, "0123456789");

    if (base == 16) {
        strcat(digit, "ABCDEF");
    } else if (base == 17) {
        strcat(digit, "abcdef");
        base = 16;
    }

    char* p = b;
    if (i < 0) {
        *p++ = '-';
        i *= -1;
    } else if (plus_sign) {
        *p++ = '+';
    } else if (!plus_sign && space_no_sign) {
        *p++ = ' ';
    }

    intmax_t shifter = i;
    do {
        ++p;
        shifter = shifter / base;
    } while (shifter);

    *p = '\0';
    do {
        *--p = digit[i % base];
        i = i / base;
    } while (i);

    int padding = padding_no - (int)strlen(b);
    if (padding < 0) padding = 0;

    if (justify) {
        while (padding--) {
            if (zero_pad) {
                b[strlen(b)] = '0';
            } else {
                b[strlen(b)] = ' ';
            }
        }

    } else {
        char a[256] = {0};
        while (padding--) {
            if (zero_pad) {
                a[strlen(a)] = '0';
            } else {
                a[strlen(a)] = ' ';
            }
        }
        strcat(a, b);
        strcpy(b, a);
    }

    return b;
}

int vfprintf( FILE* f, const char* fmt, va_list args )
{
    int chars = 0;
    char buffer[256] = { 0 };

    for( int i = 0; fmt[i]; i++ )
    {
        char specifier = '\0';
        char length = '\0';

        int length_spec = 0;
        int prec_spec = 0;
        bool left_justify = false;
        bool zero_pad = false;
        bool space_no_sign = false;
        bool alt_form = false;
        bool plus_sign = false;
        bool emode = false;
        int expo = 0;

        if( fmt[i] == '%' )
        {
            i++;

            bool ext_break = false;
            while(1)
            {
                switch(fmt[i])
                {
                    case '-':
                        left_justify = true;
                        i++;
                        break;
                    case '+':
                        plus_sign = true;
                        i++;
                        break;
                    case '#':
                        alt_form = true;
                        i++;
                        break;
                    case ' ':
                        space_no_sign = true;
                        i++;
                        break;
                    case '0':
                        zero_pad = true;
                        i++;
                        break;
                    default:
                        ext_break = true;
                        break;
                }

                if( ext_break )
                {
                    break;
                }
            }

            while( isdigit(fmt[i]) )
            {
                length_spec *= 10;
                length_spec += fmt[i] - 48;
                i++;
            }

            if( fmt[i] == '*' )
            {
                length_spec = va_arg(args, int);
                i++;
            }

            if( fmt[i] == '.' )
            {
                i++;
                while( isdigit(fmt[i]) )
                {
                    prec_spec *= 10;
                    prec_spec += fmt[i] - 48;
                    i++;
                }

                if( fmt[i] == '*' )
                {
                    prec_spec = va_arg(args, int);
                    i++;
                }
            }else
            {
                prec_spec = 6;
            }

            if( fmt[i] == 'h' || fmt[i] == 'l' || fmt[i] == 'j' ||
                fmt[i] == 'z' || fmt[i] == 't' || fmt[i] == 'L' )
            {
                length = fmt[i];
                i++;
                if( fmt[i] == 'h' )
                {
                    length = 'H';
                }else if( fmt[i] == 'l' )
                {
                    length = 'q';
                }
            }
            specifier = fmt[i];

            memset(buffer, 0, 256);

            int base = 10;
            if( specifier == 'o' )
            {
                base = 8;
                specifier = 'u';
                if(alt_form)
                {
                    chars += putc('0', f);
                }
            }
            if( specifier == 'p' )
            {
                base = 16;
                length = 'z';
                specifier = 'u';
            }
            switch( specifier )
            {
                case 'X':
                    base = 16;
                    __attribute__((fallthrough));
                case 'x':
                    base = base == 10 ? 17 : base;
                    if( alt_form )
                    {
                        char* tmp = "0x";
                        chars += fwrite(tmp, sizeof(char), 2, f);
                    }
                    __attribute__((fallthrough));
                case 'u':
                    switch( length )
                    {
                        case 0: ;
                            unsigned int uint = va_arg(args, unsigned int);
                            __int_str(uint, buffer, base, plus_sign, space_no_sign, length_spec, left_justify, zero_pad);
                            chars += fwrite(buffer, sizeof(char), strlen(buffer), f);
                            break;
                        case 'H': ;
                            unsigned char uchar = (unsigned char)va_arg(args, int);
                            __int_str(uchar, buffer, base, plus_sign, space_no_sign, length_spec, left_justify, zero_pad);
                            chars += fwrite(buffer, sizeof(char), strlen(buffer), f);
                            break;
                        case 'h': ;
                            unsigned short ushort = (unsigned short)va_arg(args, int);
                            __int_str(ushort, buffer, base, plus_sign, space_no_sign, length_spec, left_justify, zero_pad);
                            chars += fwrite(buffer, sizeof(char), strlen(buffer), f);
                            break;
                        case 'l': ;
                            unsigned long ulong = va_arg(args, unsigned long);
                            __int_str(ulong, buffer, base, plus_sign, space_no_sign, length_spec, left_justify, zero_pad);
                            chars += fwrite(buffer, sizeof(char), strlen(buffer), f);
                            break;
                        case 'q': ;
                            unsigned long long ulonglong = va_arg(args, unsigned long long);
                            __int_str(ulonglong, buffer, base, plus_sign, space_no_sign, length_spec, left_justify, zero_pad);
                            chars += fwrite(buffer, sizeof(char), strlen(buffer), f);
                            break;
                        case 'j': ;
                            uintmax_t uintmax = va_arg(args, uintmax_t);
                            __int_str(uintmax, buffer, base, plus_sign, space_no_sign, length_spec, left_justify, zero_pad);
                            chars += fwrite(buffer, sizeof(char), strlen(buffer), f);
                            break;
                        case 'z': ;
                            size_t sizet = va_arg(args, size_t);
                            __int_str(sizet, buffer, base, plus_sign, space_no_sign, length_spec, left_justify, zero_pad);
                            chars += fwrite(buffer, sizeof(char), strlen(buffer), f);
                            break;
                        case 't': ;
                            ptrdiff_t ptrdiff = va_arg(args, ptrdiff_t);
                            __int_str(ptrdiff, buffer, base, plus_sign, space_no_sign, length_spec, left_justify, zero_pad);
                            chars += fwrite(buffer, sizeof(char), strlen(buffer), f);
                            break;
                        default:
                            break;
                    }
                    break;
                case 'd':
                case 'i':
                    switch( length )
                    {
                        case 0: ;
                            int sint = va_arg(args, int);
                            __int_str(sint, buffer, base, plus_sign, space_no_sign, length_spec, left_justify, zero_pad);
                            chars += fwrite(buffer, sizeof(char), strlen(buffer), f);
                            break;
                        case 'H': ;
                            signed char schar = (signed char)va_arg(args, int);
                            __int_str(schar, buffer, base, plus_sign, space_no_sign, length_spec, left_justify, zero_pad);
                            chars += fwrite(buffer, sizeof(char), strlen(buffer), f);
                            break;
                        case 'h': ;
                            short int sshort = (short int)va_arg(args, int);
                            __int_str(sshort, buffer, base, plus_sign, space_no_sign, length_spec, left_justify, zero_pad);
                            chars += fwrite(buffer, sizeof(char), strlen(buffer), f);
                            break;
                        case 'l': ;
                            long slong = va_arg(args, long);
                            __int_str(slong, buffer, base, plus_sign, space_no_sign, length_spec, left_justify, zero_pad);
                            chars += fwrite(buffer, sizeof(char), strlen(buffer), f);
                            break;
                        case 'q': ;
                            long long slonglong = va_arg(args, long long);
                            __int_str(slonglong, buffer, base, plus_sign, space_no_sign, length_spec, left_justify, zero_pad);
                            chars += fwrite(buffer, sizeof(char), strlen(buffer), f);
                            break;
                        case 'j': ;
                            intmax_t intmax = va_arg(args, intmax_t);
                            __int_str(intmax, buffer, base, plus_sign, space_no_sign, length_spec, left_justify, zero_pad);
                            chars += fwrite(buffer, sizeof(char), strlen(buffer), f);
                            break;
                        case 'z': ;
                            size_t sizet = va_arg(args, size_t);
                            __int_str(sizet, buffer, base, plus_sign, space_no_sign, length_spec, left_justify, zero_pad);
                            chars += fwrite(buffer, sizeof(char), strlen(buffer), f);
                            break;
                        case 't': ;
                            ptrdiff_t ptrdiff = va_arg(args, ptrdiff_t);
                            __int_str(ptrdiff, buffer, base, plus_sign, space_no_sign, length_spec, left_justify, zero_pad);
                            chars += fwrite(buffer, sizeof(char), strlen(buffer), f);
                            break;
                        default:
                            break;

                    }
                    break;
                case 'c':
                    if( length == 'l' )
                    {
                        chars += putc(va_arg(args, int), f);//wint_t
                    }else
                    {
                        chars += putc(va_arg(args, int), f);
                    }
                    break;
                case 's': ;
                    char* tmp = va_arg(args, char*);
                    if( tmp == NULL )
                    {
                        tmp = "(null)";
                    }
                    chars += fwrite(tmp, sizeof(char), strlen(tmp), f);
                    break;
                case 'n':
                    switch( length )
                    {
                        case 'H':
                            *(va_arg(args, signed char*)) = (signed char)chars;
                            break;
                        case 'h':
                            *(va_arg(args, short int*)) = (short int)chars;
                            break;
                        case 0: ;
                            *(va_arg(args, int*)) = (int)chars;
                            break;
                        case 'l':
                            *(va_arg(args, long*)) = chars;
                            break;
                        case 'q':
                            *(va_arg(args, long long*)) = chars;
                            break;
                        case 'j':
                            *(va_arg(args, intmax_t*)) = chars;
                            break;
                        case 'z':
                            *(va_arg(args, size_t*)) = chars;
                            break;
                        case 't':
                            *(va_arg(args, ptrdiff_t*)) = chars;
                            break;
                        default:
                            break;
                    }
                    break;
                case 'e':
                case 'E':
                    emode = true;
                    __attribute__((fallthrough));
                case 'f':
                case 'F':
                case 'g':
                case 'G': ;
                    double floating = va_arg(args, double);

                    while( emode && floating >= 10 )
                    {
                        floating /= 10;
                        expo++;
                    }

                    int form = length_spec - prec_spec - expo - (prec_spec || alt_form ? 1 : 0);
                    if( emode )
                    {
                        form -= 4;
                    }
                    if( form < 0 )
                    {
                        form = 0;
                    }

                    __int_str((intmax_t)floating, buffer, base, plus_sign, space_no_sign, form, left_justify, zero_pad);

                    chars += fwrite(buffer, sizeof(char), strlen(buffer), f);

                    floating -= (int)floating;

                    for( int j = 0; j < prec_spec; j++ )
                    {
                        floating *= 10;
                    }
                    intmax_t dec_places = (intmax_t)(floating + 0.5);

                    if( prec_spec )
                    {
                        chars += putc('.', f);
                        __int_str(dec_places, buffer, 10, false, false, 0, false, false);
                        buffer[prec_spec] = 0;
                        chars += fwrite(buffer, sizeof(char), strlen(buffer), f);
                    }else if( alt_form )
                    {
                        chars += putc('.', f);
                    }
                    break;
                case 'a':
                case 'A':
                    break;
                default:
                    break;
            }

            if( specifier == 'e' )
            {
                char* tmp = "e+";
                chars += fwrite(tmp, sizeof(char), 2, f);
            }else if( specifier == 'E' )
            {
                char* tmp = "E+";
                chars += fwrite(tmp, sizeof(char), 2, f);
            }

            if( specifier == 'e' || specifier == 'E' )
            {
                __int_str(expo, buffer, 10, false, false, 2, false, true);
                chars += fwrite(buffer, sizeof(char), strlen(buffer), f);
            }
        }else
        {
            chars += putc(fmt[i], f);
        }
    }
    return chars;
}
