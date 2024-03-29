#ifndef _KERNEL_SERIAL_H
#define _KERNEL_SERIAL_H

#include <stddef.h> // size_t
#include <kernel/kernel.h> // asm volatile macro, debug
#include <string.h> // strlen

#define _inline inline __attribute__((always_inline))

static _inline unsigned int inportl( unsigned short _port )
{
    unsigned int rv;
    asm volatile("inl %1, %0" : "=a"(rv):"dN"(_port));
    return rv;
}

static _inline void outportl( unsigned short _port, unsigned int _data )
{
    asm volatile ("outl %1, %0" : : "dN" (_port), "a" (_data));
}

static _inline unsigned short inports( unsigned short _port )
{
    unsigned short rv;
    asm volatile("inw %1, %0" : "=a" (rv):"dN"(_port));
    return rv;
}

static _inline void outports( unsigned short _port, unsigned short _data )
{
    asm volatile("outw %1, %0" :: "dN"(_port), "a"(_data));
}

static _inline unsigned char inportb( unsigned short _port )
{
    unsigned char rv;
    asm volatile("inb %1, %0" : "=a"(rv):"dN"(_port));
    return rv;
}

static _inline void outportb( unsigned short _port, unsigned char _data )
{
    asm volatile("outb %1, %0" :: "dN"(_port), "a"(_data));
}

static _inline void inportsm( unsigned short port, unsigned char* data, unsigned long size )
{
    asm volatile("rep insw" : "+D"(data), "+c"(size):"d"(port):"memory");
}

static _inline void outportsm( unsigned short port, unsigned char* data, unsigned long size )
{
    asm volatile("rep outsw" : "+S" (data), "+c" (size) : "d" (port));
}

static _inline void debug_log( const char* str )
{
    if( !CHECK_FLAG(debug, 0) )
    {
        return;
    }
    size_t len = strlen(str);
    
    for( size_t i = 0; i < len; i++ )
    {
        if( str[i] == '\0' ) break;
        while( (inportb(0x3F8 + 5) & 0x20) == 0 );
        outportb(0x3F8, (unsigned char)str[i]);
    }
    
    while( (inportb(0x3F8 + 5) & 0x20) == 0 );
    outportb(0x3F8, '\r');
    while( (inportb(0x3F8 + 5) & 0x20) == 0 );
    outportb(0x3F8, '\n');
}

void debug_logf( char* str, const char* format, ... );

#endif
