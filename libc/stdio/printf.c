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

static bool hexprint( unsigned long i, size_t length )
{
    char str[length+1];
    char* s;
    int t;
    unsigned long u = i;

    if( i == 0 )
    {
        str[0] = '0';
        str[1] = '\0';
        return print(str, strlen(str));
    }

    s = str + length;
    *s = '\0';
    
    while(u)
    {
        t = u % 16;
        if( t >= 10 )
            t += 'A' - '0' - 10;
        *--s = t + '0';
        u /= 16;
    }

    return print(str, strlen(str));
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

static bool sprint( char*s, const char* data, size_t length )
{
    for( size_t i = 0; i < length; i++ )
        s[i]=data[i];

    return true;
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

int sprintf( char* s, const char* restrict format, ... )
{
    va_list parameters;
    va_start(parameters, format);

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
			size_t len = strlen(str);
			if (!sprint(s + written, str, len))
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
    
    va_end(parameters);

    return written;
}
