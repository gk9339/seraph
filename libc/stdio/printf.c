#include <limits.h>
#include <stdbool.h>
#include <stdarg.h>
#include <stdio.h>
#include <string.h>

static bool print( const char* data, size_t length )
{
	const unsigned char* bytes = (const unsigned char*)data;

    for( size_t i = 0; i < length; i++ )
		if (putchar(bytes[i]) == EOF)
			return false;
	
    return true;
}

static bool sprint( char* s, const char* data, size_t length )
{
    for( size_t i = 0; i < length; i++ )
        s[i]=data[i];

    return true;
}

static bool hexsprint( char* s, unsigned long data, size_t length )
{
    char str[length+1];
    char* i;
    int t;
    unsigned long u = data;

    if( data == 0 )
    {
        str[0] = '0';
        str[1] = '\0';
        if( s != NULL )
        {
            return sprint(s, str, strlen(str));
        }else
        {
            return print(str, strlen(str));
        }
    }

    i = str + length;
    *i = '\0';
    
    while(u)
    {
        t = u % 16;
        if( t >= 10 )
            t += 'A' - '0' - 10;
        *--i = (char)(t + '0');
        u /= 16;
    }

    if( s != NULL )
    {
        return sprint(s, str, strlen(str));
    }else
    {
        return print(str, strlen(str));
    }
    
}

static bool hexprint( unsigned long data, size_t length )
{
    return hexsprint(NULL, data, length);
}

static bool decsprint( char* s,  unsigned long data, size_t length )
{
    char str[length+1];
    char* i;
    int t;
    unsigned long u = data;

    if( data == 0 )
    {
        str[0] = '0';
        str[1] = '\0';
        if( s != NULL )
        {
            return sprint(s, str, strlen(str));
        }else
        {
            return print(str, strlen(str));
        }
    }

    i = str + length;
    *i = '\0';
    
    while(u)
    {
        t = u % 10;
        *--i = (char)(t + '0');
        u /= 10;
    }

    if( s != NULL )
    {
        return sprint(s, str, strlen(str));
    }else
    {
        return print(str, strlen(str));
    }
}

static bool decprint( unsigned long data, size_t length )
{
    return decsprint(NULL, data, length);
}

static size_t hexlen( unsigned long hex )
{
    size_t len = 0;

    while( hex != 0 )
    {
        len++;
        hex /= 16;
    }

    return len;
}

static size_t declen( unsigned long dec )
{
    size_t len = 0;
    if( dec == 0 ) return 1;

    while( dec!= 0 )
    {
        len++;
        dec /= 10;
    }

    return len;
}

int printf( const char* restrict format, ... ) 
{
	va_list parameters;
	va_start(parameters, format);

	int written = 0;

	while( *format != '\0' ) 
    {
		size_t maxrem = INT_MAX - written;

		if( format[0] != '%' || format[1] == '%' ) 
        {
			if( format[0] == '%' )
				format++;
			size_t amount = 1;
			while( format[amount] && format[amount] != '%' )
				amount++;
			if( maxrem < amount ) 
            {
				// TODO: Set errno to EOVERFLOW.
				return -1;
			}
			if( !print(format, amount) )
				return -1;
			format += amount;
			written += amount;
			continue;
		}

		const char* format_begun_at = format++;

		if( *format == 'c' ) 
        {
			format++;
			char c = (char)va_arg(parameters, int /* char promotes to int */);
			if( !maxrem ) 
            {
				// TODO: Set errno to EOVERFLOW.
				return -1;
			}
			if( !print(&c, sizeof(c)) )
				return -1;
			written++;
		}else if( *format == 's' ) 
        {
			format++;
			const char* str = va_arg(parameters, const char*);
			size_t len = strlen(str);
			if( maxrem < len ) 
            {
				// TODO: Set errno to EOVERFLOW.
				return -1;
			}
			if( !print(str, len) )
				return -1;
			written += len;
		}else if( *format == 'x' )
        {
			format++;
			unsigned long hex = va_arg(parameters, unsigned long);
			size_t len = hexlen(hex);
			if (maxrem < len) 
            {
				// TODO: Set errno to EOVERFLOW.
				return -1;
			}
			if (!hexprint(hex, len))
				return -1;
			written += len;
		}else if( *format == 'd' )
        {
			format++;
			unsigned long dec = va_arg(parameters, unsigned long);
			size_t len = declen(dec);
			if (maxrem < len) 
            {
				// TODO: Set errno to EOVERFLOW.
				return -1;
			}
			if (!decprint(dec, len))
				return -1;
			written += len;
		}else 
        {
			format = format_begun_at;
			size_t len = strlen(format);
			if (maxrem < len) 
            {
				// TODO: Set errno to EOVERFLOW.
				return -1;
			}
			if (!print(format, len))
				return -1;
			written += len;
			format += len;
		}
	}

	va_end(parameters);
	return written;
}

int vsprintf( char* s, const char* restrict format, va_list parameters )
{
    int written = 0;

	while (*format != '\0') 
    {
		if (format[0] != '%' || format[1] == '%') 
        {
			if (format[0] == '%')
				format++;
			size_t amount = 1;
			while (format[amount] && format[amount] != '%')
				amount++;
			if (!sprint(s + written, format, amount))
				return -1;
			format += amount;
			written += amount;
			continue;
		}

		const char* format_begun_at = format++;

		if (*format == 'c') 
        {
			format++;
			char c = (char) va_arg(parameters, int /* char promotes to int */);
			if (!sprint(s + written, &c, sizeof(c)))
				return -1;
			written++;
		}else if (*format == 's') 
        {
			format++;
			const char* str = va_arg(parameters, const char*);
            size_t len;
            if( str != NULL )
            {
    			len = strlen(str);
	    		if( !sprint(s + written, str, len) )
		    		return -1;
            }else
            {
                len = 6;
                if( !sprint(s + written, "(null)", len) )
                    return -1;
            }
			written += len;
		}else if( *format == 'x' )
        {
			format++;
			unsigned long hex = va_arg(parameters, unsigned long);
			size_t len = hexlen(hex);
			if (!hexsprint(s + written, hex, len))
				return -1;
			written += len;
		}else if( *format == 'd' )
        {
			format++;
			unsigned long dec = va_arg(parameters, unsigned long);
			size_t len = declen(dec);
			if (!decsprint(s + written, dec, len))
				return -1;
			written += len;
		}else 
        {
			format = format_begun_at;
			size_t len = strlen(format);
			if (!sprint(s + written, format, len))
				return -1;
			written += len;
			format += len;
		}
	}
    

    *(s + written) = '\0';

    return written;
}

int sprintf( char* buf, const char* restrict format, ... )
{
    va_list parameters;
    va_start(parameters, format);
    int written = vsprintf(buf, format, parameters);
    va_end(parameters);

    return written;
}
