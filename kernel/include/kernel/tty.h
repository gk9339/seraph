#ifndef _KERNEL_TTY_H
#define _KERNEL_TTY_H

#include <stddef.h> /* size_t */
#include <kernel/fs.h> /* fs_node_t */
#include <sys/termios.h> /* struct termios */

void terminal_initialize( void );
void terminal_clear( void );
void terminal_scroll( size_t rows );
void terminal_putchar( char c );
void terminal_write( const char* data, size_t size );
void terminal_writestring( const char* data );
void terminal_setcolor( uint8_t );

#endif
