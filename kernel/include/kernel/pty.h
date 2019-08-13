#ifndef _KERNEL_PTY_H
#define _KERNEL_PTY_H

#include <stddef.h> /* size_t */
#include <kernel/fs.h> /* fs_node_t */
#include <sys/termios.h> /* struct termios */
#include <sys/ioctl.h> /* struct winsize */
#include <kernel/cbuffer.h> /* circular_buffer_t */

typedef struct pty
{
    int name;

    fs_node_t* master;
    fs_node_t* slave;

    struct winsize pty_winsize;

    struct termios pty_termios;

    circular_buffer_t* in;
    circular_buffer_t* out;

    char* canon_buffer;
    size_t canon_bufsize;
    size_t canon_buflen;

    pid_t controlling_process;
    pid_t foreground_process;

    void (*write_in)(struct pty*, uint8_t);
    void (*write_out)(struct pty*, uint8_t);

    int next_is_verbatim;

    void (*fill_name)(struct pty*, char*);
} pty_t;

void pty_output_process_slave( pty_t*, uint8_t );
void pty_output_process( pty_t*, uint8_t );
void pty_input_process( pty_t*, uint8_t );

int pty_create( void*, fs_node_t**, fs_node_t** );
pty_t* pty_new( struct winsize* );

#endif
