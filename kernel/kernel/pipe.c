#include <kernel/pipe.h>
#include <kernel/process.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <kernel/signal.h>
#include <kernel/task.h>
#include <kernel/spinlock.h>
#include <kernel/cmos.h>

static inline size_t pipe_unread( pipe_device_t* pipe )
{
	if( pipe->read_ptr == pipe->write_ptr )
    {
		return 0;
	}
	if( pipe->read_ptr > pipe->write_ptr )
    {
		return (pipe->size - pipe->read_ptr) + pipe->write_ptr;
	}else
    {
		return (pipe->write_ptr - pipe->read_ptr);
	}
}

int pipe_size( fs_node_t* node )
{
	pipe_device_t* pipe = (pipe_device_t*)node->device;
	
    return pipe_unread(pipe);
}

static inline size_t pipe_available( pipe_device_t* pipe )
{
	if( pipe->read_ptr == pipe->write_ptr )
    {
		return pipe->size - 1;
	}

	if( pipe->read_ptr > pipe->write_ptr )
    {
		return pipe->read_ptr - pipe->write_ptr - 1;
	}else
    {
		return (pipe->size - pipe->write_ptr) + pipe->read_ptr - 1;
	}
}

int pipe_unsize( fs_node_t* node )
{
	pipe_device_t* pipe = (pipe_device_t*)node->device;

    return pipe_available(pipe);
}

static inline void pipe_increment_read( pipe_device_t* pipe )
{
	pipe->read_ptr++;
	if( pipe->read_ptr == pipe->size )
    {
		pipe->read_ptr = 0;
	}
}

static inline void pipe_increment_write( pipe_device_t* pipe )
{
	pipe->write_ptr++;
	if( pipe->write_ptr == pipe->size )
    {
		pipe->write_ptr = 0;
	}
}

static inline void pipe_increment_write_by( pipe_device_t* pipe, size_t amount )
{
	pipe->write_ptr = (pipe->write_ptr + amount) % pipe->size;
}

static void pipe_alert_waiters( pipe_device_t* pipe )
{
	if( pipe->alert_waiters )
    {
		while( pipe->alert_waiters->head )
        {
			node_t* node = list_dequeue(pipe->alert_waiters);
			process_t* p = node->value;
			process_alert_node(p, pipe);
			free(node);
		}
	}
}

uint32_t read_pipe( fs_node_t*node, uint32_t offset, uint32_t size, uint8_t* buffer )
{
	/* Retreive the pipe object associated with this file node */
	pipe_device_t* pipe = (pipe_device_t*)node->device;

	if( pipe->dead )
    {
		send_signal(getpid(), SIGPIPE, 1);
		return 0;
	}

	size_t collected = 0;
	while( collected == 0 )
    {
		spin_lock(pipe->lock_read);
		while( pipe_unread(pipe) > 0 && collected < size )
        {
			buffer[collected] = pipe->buffer[pipe->read_ptr];
			pipe_increment_read(pipe);
			collected++;
		}
		spin_unlock(pipe->lock_read);
		wakeup_queue(pipe->wait_queue_writers);
		/* Deschedule and switch */
		if( collected == 0 )
        {
			sleep_on(pipe->wait_queue_readers);
		}
	}

	return collected;
}

uint32_t write_pipe( fs_node_t*node, uint32_t offset, uint32_t size, uint8_t* buffer )
{
	/* Retreive the pipe object associated with this file node */
	pipe_device_t* pipe = (pipe_device_t*)node->device;

	if( pipe->dead )
    {
		send_signal(getpid(), SIGPIPE, 1);
		return 0;
	}

	size_t written = 0;
	while( written < size )
    {
		spin_lock(pipe->lock_write);
		while( pipe_available(pipe) > 0 && written < size )
        {
			pipe->buffer[pipe->write_ptr] = buffer[written];
			pipe_increment_write(pipe);
			written++;
		}

		spin_unlock(pipe->lock_write);
		wakeup_queue(pipe->wait_queue_readers);
		pipe_alert_waiters(pipe);
		if( written < size )
        {
			sleep_on(pipe->wait_queue_writers);
		}
	}

	return written;
}

void open_pipe( fs_node_t* node, unsigned int flags )
{
	/* Retreive the pipe object associated with this file node */
	pipe_device_t* pipe = (pipe_device_t*)node->device;

	/* Add a reference */
	pipe->refcount++;

	return;
}

void close_pipe( fs_node_t* node )
{
	/* Retreive the pipe object associated with this file node */
	pipe_device_t* pipe = (pipe_device_t*)node->device;

	/* Drop one reference */
	pipe->refcount--;

	/* Check the reference count number */
	if( pipe->refcount == 0 )
    {
		/* No other references exist, free the pipe (but not its buffer) */
		free(pipe->buffer);
		list_free(pipe->wait_queue_readers);
        list_free(pipe->wait_queue_writers);
		free(pipe->wait_queue_readers);
        free(pipe->wait_queue_writers);
		free(pipe);
		/* And let the creator know there are no more references */
		node->device = 0;
	}

	return;
}

static int pipe_check( fs_node_t* node )
{
	pipe_device_t* pipe = (pipe_device_t*)node->device;

	if( pipe_unread(pipe) > 0 )
    {
		return 0;
	}

	return 1;
}

static int pipe_wait( fs_node_t* node, void* process )
{
	pipe_device_t* pipe = (pipe_device_t*)node->device;

	if( !pipe->alert_waiters )
    {
		pipe->alert_waiters = list_create();
	}

	if( !list_find(pipe->alert_waiters, process) )
    {
		list_insert(pipe->alert_waiters, process);
	}
	list_insert(((process_t*)process)->node_waits, pipe);

	return 0;
}

fs_node_t* make_pipe( size_t size )
{
	fs_node_t* fnode = malloc(sizeof(fs_node_t));
	pipe_device_t* pipe = malloc(sizeof(pipe_device_t));
	memset(fnode, 0, sizeof(fs_node_t));
	memset(pipe, 0, sizeof(pipe_device_t));

	fnode->device = 0;
	fnode->name[0] = '\0';
	sprintf(fnode->name, "[pipe]");
	fnode->uid   = 0;
	fnode->gid   = 0;
	fnode->mask  = 0666;
	fnode->flags = FS_PIPE;
	fnode->read  = read_pipe;
	fnode->write = write_pipe;
	fnode->open  = open_pipe;
	fnode->close = close_pipe;
	fnode->readdir = NULL;
	fnode->finddir = NULL;
	fnode->ioctl   = NULL; /* TODO ioctls for pipes? maybe */
	fnode->get_size = pipe_size;

	fnode->selectcheck = pipe_check;
	fnode->selectwait  = pipe_wait;

	fnode->atime = now();
	fnode->mtime = fnode->atime;
	fnode->ctime = fnode->atime;

	fnode->device = pipe;

	pipe->buffer    = malloc(size);
	pipe->write_ptr = 0;
	pipe->read_ptr  = 0;
	pipe->size      = size;
	pipe->refcount  = 0;
	pipe->dead      = 0;

	spin_init(pipe->lock_read);
	spin_init(pipe->lock_write);

	pipe->wait_queue_writers = list_create();
	pipe->wait_queue_readers = list_create();

	return fnode;
}
