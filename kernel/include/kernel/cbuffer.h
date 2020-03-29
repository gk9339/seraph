#ifndef _KERNEL_CBUFFER_H
#define _KERNEL_CBUFFER_H

#include <stddef.h> // size_t
#include <stdint.h> // intN_t
#include <list.h> // list_t
#include <kernel/fs.h> // fs_node_t

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

size_t circular_buffer_unread( circular_buffer_t* cbuffer ); // Amount of unread bytes in cbuffer
size_t circular_buffer_size( fs_node_t* cbuffer ); // Size of cbuffer
size_t circular_buffer_available( circular_buffer_t* cbuffer ); // Available space in cbuffer
size_t circular_buffer_read( circular_buffer_t* cbuffer, size_t size, uint8_t* buffer ); // Read (size) bytes into *buffer from cbuffer
size_t circular_buffer_write( circular_buffer_t* cbuffer, size_t size, uint8_t* buffer ); // Write (size) bytes from *buffer into cbuffer

circular_buffer_t* circular_buffer_create( size_t size ); // Initialize cbuffer
void circular_buffer_destroy( circular_buffer_t* cbuffer ); // Free cbuffer buffers, wakeup waiting processes
void circular_buffer_interrupt( circular_buffer_t* cbuffer ); // Interrupt reading/writing processes
void circular_buffer_alert_waiters( circular_buffer_t* cbuffer ); // Alert waiting processes
void circular_buffer_selectwait( circular_buffer_t* cbuffer, void* process ); // Add process to alert_waiters list

#endif
