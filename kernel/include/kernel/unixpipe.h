#ifndef _KERNEL_UNIXPIPE_H
#define _KERNEL_UNIXPIPE_H

#include <kernel/fs.h> // fs_node_t
#include <kernel/cbuffer.h> // circular_buffer_t

#define UNIX_PIPE_BUFFER 512

struct unix_pipe
{
    fs_node_t* read_end;
    fs_node_t* write_end;

    volatile int read_closed;
    volatile int write_closed;

    circular_buffer_t* buffer;
};

int make_unix_pipe( fs_node_t** pipes );

#endif
