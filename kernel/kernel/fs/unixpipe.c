#include <kernel/unixpipe.h>
#include <stdlib.h>
#include <stdint.h>
#include <list.h>
#include <string.h>
#include <stdio.h>
#include <kernel/signal.h>
#include <kernel/task.h>
#include <kernel/process.h>

static uint32_t read_unixpipe( fs_node_t* node, uint32_t offset __attribute__((unused)), uint32_t size, uint8_t* buffer )
{
    struct unix_pipe* self = node->device;
    size_t read = 0;

    while( read < size )
    {
        if( self->write_closed && !circular_buffer_unread(self->buffer) )
        {
            return read;
        }
        size_t ret = circular_buffer_read(self->buffer, 1, buffer + read);
        if( ret && *((char*)(buffer + read)) == '\n' )
        {
            return read + ret;
        }
        read += ret;
    }

    return read;
}

static uint32_t write_unixpipe( fs_node_t* node, uint32_t offset __attribute__((unused)), uint32_t size, uint8_t* buffer )
{
    struct unix_pipe* self = node->device;
    size_t written = 0;

    while( written < size )
    {
        if( self->read_closed )
        {
            send_signal(getpid(), SIGPIPE, 1);

            return written;
        }
        size_t ret = circular_buffer_write(self->buffer, 1, buffer + written);
        written += ret;
    }

    return written;
}

static void close_read_pipe( fs_node_t* node )
{
    struct unix_pipe* self = node->device;

    self->read_closed = 1;
    if( !self->write_closed )
    {
        circular_buffer_interrupt(self->buffer);
    }else
    {
        circular_buffer_alert_waiters(self->buffer);
    }
}

static void close_write_pipe( fs_node_t* node )
{
    struct unix_pipe* self = node->device;

    self->write_closed = 1;
    if( !self->read_closed )
    {
        circular_buffer_interrupt(self->buffer);
        if( !circular_buffer_unread(self->buffer) )
        {
            circular_buffer_alert_waiters(self->buffer);
        }
    }else
    {
        circular_buffer_destroy(self->buffer);
    }
}

static int check_pipe( fs_node_t* node )
{
    struct unix_pipe* self = node->device;

    if( circular_buffer_unread(self->buffer) > 0 )
    {
        return 0;
    }

    if( self->write_closed )
    {
        return 0;
    }

    return 1;
}

static int wait_pipe( fs_node_t* node, void* process )
{
    struct unix_pipe* self = node->device;
    circular_buffer_selectwait(self->buffer, process);
    
    return 0;
}

int make_unix_pipe( fs_node_t** pipes )
{
    pipes[0] = malloc(sizeof(fs_node_t));
    pipes[1] = malloc(sizeof(fs_node_t));

    memset(pipes[0], 0, sizeof(fs_node_t));
    memset(pipes[1], 0, sizeof(fs_node_t));

    sprintf(pipes[0]->name, "[pipe:read]");
    sprintf(pipes[1]->name, "[pipe:write]");

    pipes[0]->mask = S_IROTH | S_IWOTH | S_IRGRP | S_IWGRP | S_IRUSR | S_IWUSR;
    pipes[1]->mask = S_IROTH | S_IWOTH | S_IRGRP | S_IWGRP | S_IRUSR | S_IWUSR;

    pipes[0]->type = FS_PIPE;
    pipes[1]->type = FS_PIPE;

    pipes[0]->read = read_unixpipe;
    pipes[1]->write = write_unixpipe;

    pipes[0]->close = close_read_pipe;
    pipes[1]->close = close_write_pipe;

    pipes[0]->selectcheck = check_pipe;
    pipes[0]->selectwait = wait_pipe;

    struct unix_pipe* internals = malloc(sizeof(struct unix_pipe));
    internals->read_end = pipes[0];
    internals->write_end = pipes[1];
    internals->read_closed = 0;
    internals->write_closed = 0;
    internals->buffer = circular_buffer_create(UNIX_PIPE_BUFFER);

    pipes[0]->device = internals;
    pipes[1]->device = internals;

    return 0;
}
