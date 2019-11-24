#include <kernel/cbuffer.h>
#include <kernel/spinlock.h>
#include <kernel/process.h>
#include <stdlib.h>

size_t circular_buffer_unread( circular_buffer_t* cbuffer )
{
    if( cbuffer->read_ptr == cbuffer->write_ptr )
    {
        return 0;
    }
    
    if( cbuffer->read_ptr > cbuffer->write_ptr )
    {
        return (cbuffer->size - cbuffer->read_ptr) + cbuffer->write_ptr;
    }else
    {
        return cbuffer->write_ptr - cbuffer->read_ptr;
    }
}

size_t circular_buffer_size( fs_node_t* node )
{
    circular_buffer_t* cbuffer = (circular_buffer_t*)node->device;
    
    return circular_buffer_unread(cbuffer);
}

size_t circular_buffer_available( circular_buffer_t* cbuffer )
{
    if( cbuffer->read_ptr == cbuffer->write_ptr )
    {
        return cbuffer->size - 1;
    }

    if( cbuffer->read_ptr > cbuffer->write_ptr )
    {
        return cbuffer->read_ptr - cbuffer->write_ptr - 1;
    }else
    {
        return (cbuffer->size - cbuffer->write_ptr) + cbuffer->read_ptr - 1;
    }
}

static inline void circular_buffer_increment_read( circular_buffer_t* cbuffer )
{
    cbuffer->read_ptr++;
    if( cbuffer->read_ptr == cbuffer->size )
    {
        cbuffer->read_ptr = 0;
    }
}

static inline void circular_buffer_increment_write( circular_buffer_t* cbuffer )
{
    cbuffer->write_ptr++;
    if( cbuffer->write_ptr == cbuffer->size )
    {
        cbuffer->write_ptr = 0;
    }
}

size_t circular_buffer_read( circular_buffer_t* cbuffer, size_t size, uint8_t* buffer )
{
    size_t collected = 0;
    while( !collected )
    {
        spin_lock(cbuffer->lock);
        if(!cbuffer){while(1){;}};
        while( circular_buffer_unread(cbuffer) > 0 && collected < size )
        {
            buffer[collected] = cbuffer->buffer[cbuffer->read_ptr];
            circular_buffer_increment_read(cbuffer);
            collected++;
        }
        spin_unlock(cbuffer->lock);

        wakeup_queue(cbuffer->wait_queue_writers);
        if( !collected )
        {
            if( sleep_on(cbuffer->wait_queue_readers) && cbuffer->internal_stop )
            {
                cbuffer->internal_stop = 0;
                break;
            }
        }
    }
    wakeup_queue(cbuffer->wait_queue_writers);
    
    return collected;
}

size_t circular_buffer_write( circular_buffer_t* cbuffer, size_t size, uint8_t* buffer )
{
    size_t written = 0;
    while( written < size )
    {
        spin_lock(cbuffer->lock);
        while( circular_buffer_available(cbuffer) > 0 && written < size )
        {
            cbuffer->buffer[cbuffer->write_ptr] = buffer[written];
            circular_buffer_increment_write(cbuffer);
            written++;
        }
        spin_unlock(cbuffer->lock);

        wakeup_queue(cbuffer->wait_queue_readers);
        circular_buffer_alert_waiters(cbuffer);
        if( written < size )
        {
            if( cbuffer->discard )
            {
                break;
            }
            if( sleep_on(cbuffer->wait_queue_writers) && cbuffer->internal_stop )
            {
                cbuffer->internal_stop = 0;
                break;
            }
        }
    }
    wakeup_queue(cbuffer->wait_queue_readers);
    circular_buffer_alert_waiters(cbuffer);

    return written;
}

circular_buffer_t* circular_buffer_create( size_t size )
{
    circular_buffer_t* cbuffer = malloc(sizeof(circular_buffer_t));

    cbuffer->buffer = malloc(size);
    cbuffer->write_ptr = 0;
    cbuffer->read_ptr = 0;
    cbuffer->size = size;
    cbuffer->alert_waiters = NULL;

    spin_init(cbuffer->lock);

    cbuffer->internal_stop = 0;
    cbuffer->discard = 0;

    cbuffer->wait_queue_readers = list_create();
    cbuffer->wait_queue_writers = list_create();

    return cbuffer;
}

void circular_buffer_destroy( circular_buffer_t* cbuffer )
{
    free(cbuffer->buffer);

    wakeup_queue(cbuffer->wait_queue_readers);
    wakeup_queue(cbuffer->wait_queue_writers);
    circular_buffer_alert_waiters(cbuffer);

    list_free(cbuffer->wait_queue_readers);
    list_free(cbuffer->wait_queue_writers);

    if( cbuffer->alert_waiters )
    {
        list_free(cbuffer->alert_waiters);
        free(cbuffer->alert_waiters);
    }
}

void circular_buffer_interrupt( circular_buffer_t* cbuffer )
{
    cbuffer->internal_stop = 1;
    wakeup_queue_interrupted(cbuffer->wait_queue_readers);
    wakeup_queue_interrupted(cbuffer->wait_queue_writers);
}

void circular_buffer_alert_waiters( circular_buffer_t* cbuffer )
{
    if( cbuffer->alert_waiters )
    {
        while( cbuffer->alert_waiters->head )
        {
            node_t* node = list_dequeue(cbuffer->alert_waiters);
            process_t* proc = node->value;
            process_alert_node(proc, cbuffer);
            free(node);
        }
    }
}

void circular_buffer_selectwait( circular_buffer_t* cbuffer, void* process )
{
    if( !cbuffer->alert_waiters )
    {
        cbuffer->alert_waiters = list_create();
    }

    if( !list_find(cbuffer->alert_waiters, process) )
    {
        list_insert(cbuffer->alert_waiters, process);
    }

    list_insert(((process_t*)process)->node_waits, cbuffer);
}
