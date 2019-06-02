#ifndef _KERNEL_CBUFFER_H
#define _KERNEL_CBUFFER_H

#include <stddef.h>
#include <sys/types.h>
#include <list.h>
#include <kernel/fs.h>

typedef struct
{
    unsigned char* buffer;
    size_t write_ptr;
    size_t read_ptr;
    size_t size;
    volatile int lock[2];
    list_t* wait_queue_readers;
    list_t* wait_queue_writers;
    int internal_stop;
    list_t* alert_waiters;
    int discard;
} circular_buffer_t;

size_t circular_buffer_unread( circular_buffer_t* cbuffer );
size_t circular_buffer_size( fs_node_t* cbuffer );
size_t circular_buffer_available( circular_buffer_t* cbuffer );
size_t circular_buffer_read( circular_buffer_t* cbuffer, size_t size, uint8_t* buffer );
size_t circular_buffer_write( circular_buffer_t* cbuffer, size_t size, uint8_t* buffer );

circular_buffer_t* circular_buffer_create( size_t size );
void circular_buffer_destroy( circular_buffer_t* cbuffer );
void circular_buffer_interrupt( circular_buffer_t* cbuffer );
void circular_buffer_alert_waiters( circular_buffer_t* cbuffer );
void circular_buffer_selectwait( circular_buffer_t* cbuffer, void* process );

#endif
