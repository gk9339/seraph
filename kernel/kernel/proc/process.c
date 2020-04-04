#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <kernel/process.h>
#include <kernel/irq.h>
#include <kernel/shm.h>
#include <kernel/serial.h>
#include <kernel/task.h>
#include <kernel/kernel.h>
#include <kernel/mem.h>
#include <kernel/spinlock.h>
#include <kernel/bitset.h>
#include <kernel/signal.h>
#include <kernel/timer.h>
#include <kernel/cmos.h>
#include <kernel/kconfig.h>
#include <sys/types.h>
#include <sys/signals.h>
#include <sys/wait.h>
#include <errno.h>
#include <list.h>
#include <tree.h>

tree_t* process_tree;
list_t* process_list;
list_t* process_queue;
list_t* sleep_queue;
volatile process_t* current_process = NULL;
process_t* kernel_idle_task = NULL;

static spin_lock_t tree_lock = { 0 };
static spin_lock_t process_queue_lock = { 0 };
static spin_lock_t wait_lock_temp = { 0 };
static spin_lock_t sleep_lock = { 0 };

static bitset_t pid_set;

char* default_name = "[no_name]";

static int _next_pid = 2;

int is_valid_process( process_t* process )
{
    foreach(node, process_list)
    {
        if( node->value == process )
        {
            return 1;
        }
    }

    return 0;
}

void initialize_process_tree( void )
{
    process_tree = tree_create();
    process_list = list_create();
    process_queue = list_create();
    sleep_queue = list_create();

    bitset_init(&pid_set, MAX_PID / 8);
    
    bitset_set(&pid_set, 0);
    bitset_set(&pid_set, 1);
}

process_t* next_ready_process( void )
{
    char debug_str[128];
    if( !process_available() )
    {
        return kernel_idle_task;
    }

    if( process_queue->head->owner != process_queue )
    {
        sprintf(debug_str, "ERROR: process queue head node 0x%x has owner 0x%x but process queue is 0x%x", 
                process_queue->head, process_queue->head->owner, process_queue);
        debug_log(debug_str);

        process_t* proc = process_queue->head->value;

        sprintf(debug_str, "PID asssociated with this node is %d", proc->id);
        debug_log(debug_str);
    }
    node_t* np = list_dequeue(process_queue);
    process_t* next = np->value;

    return next;
}

void make_process_ready( process_t* proc )
{
    if( proc->sleep_node.owner != NULL )
    {
        if( proc->sleep_node.owner == sleep_queue )
        {
            if( proc->timed_sleep_node )
            {
                int_disable();

                spin_lock(sleep_lock);
                list_delete(sleep_queue, proc->timed_sleep_node);
                spin_unlock(sleep_lock);

                int_resume();

                proc->sleep_node.owner = NULL;
                free(proc->timed_sleep_node->value);
            }
        }else
        {
            proc->sleep_interrupted = 1;
            spin_lock(wait_lock_temp);
            list_delete((list_t*)proc->sleep_node.owner, &proc->sleep_node);
            spin_unlock(wait_lock_temp);
        }
    }
    if( proc->sched_node.owner )
    {
        debug_log("Can't make process ready without removing from owner list: %d");
        return;
    }
    spin_lock(process_queue_lock);
    list_append(process_queue, &proc->sched_node);
    spin_unlock(process_queue_lock);
}

void delete_process( process_t* proc )
{
    tree_node_t* entry = proc->tree_entry;

    if( !entry ) return;

    if( process_tree->root == entry )
    {
        return;
    }

    spin_lock(tree_lock);
    int has_children = entry->children->length;
    tree_remove_reparent_root(process_tree, entry);
    list_delete(process_list, list_find(process_list, proc));
    spin_unlock(tree_lock);

    if( has_children )
    {
        process_t* init = process_tree->root->value;
        wakeup_queue(init->wait_queue);
    }

    bitset_clear(&pid_set, proc->id);

    free(proc);
}

process_t* spawn_init( void )
{
    process_t* init = malloc(sizeof(process_t));
    tree_set_root(process_tree, (void*)init);

    init->tree_entry = process_tree->root;
    init->id = 1;
    init->group = 0;
    init->job = 1;
    init->session = 1;
    init->name = strdup("init");
    init->cmdline = NULL;
    init->user = 0;
    init->real_user = 0;
    init->user_group = 0;
    init->mask = 022;
    init->status = 0;
    init->fds = malloc(sizeof(fd_table_t));
    init->fds->length = 0;
    init->fds->capacity = 4;
    init->fds->entries = malloc(init->fds->capacity * sizeof(fs_node_t*));
    init->fds->flags = malloc(init->fds->capacity * sizeof(int));
    init->fds->offsets = malloc(init->fds->capacity * sizeof(uint64_t));

    init->wd_node = clone_fs(fs_root);
    init->wd_name = strdup("/");

    init->image.entry = 0;
    init->image.heap = 0;
    init->image.heap_actual = 0;
    init->image.stack = initial_esp + 1;
    init->image.user_stack = 0;
    init->image.size = 0;
    init->image.shm_heap = SHM_START;

    spin_init(init->image.lock);

    init->finished = 0;
    init->suspended = 0;
    init->started = 1;
    init->running = 1;
    init->wait_queue = list_create();
    init->shm_mappings = list_create();
    init->signal_queue = list_create();
    init->signal_kstack = NULL;

    init->sched_node.prev = NULL;
    init->sched_node.next = NULL;
    init->sched_node.value = init;

    init->sleep_node.prev = NULL;
    init->sleep_node.next = NULL;
    init->sleep_node.value = init;

    init->timed_sleep_node = NULL;

    init->is_daemon = 0;

    set_process_environment(init, current_directory);

    init->description = strdup("init process");
    list_insert(process_list, (void*)init);

    return init;
}

static void _kidle( void )
{
    while(1)
    {
        int_enable();
        PAUSE;
    }
}

process_t* spawn_kidle( void )
{
    process_t* idle = malloc(sizeof(process_t));
    memset(idle, 0x00, sizeof(process_t));
    idle->id = -1;
    idle->name = strdup("[kidle]");
    idle->is_daemon = 1;

    idle->image.stack = (uintptr_t)malloc(KERNEL_STACK_SIZE) + KERNEL_STACK_SIZE;
    idle->thread.eip = (uintptr_t)&_kidle;
    idle->thread.esp = idle->image.stack;
    idle->thread.ebp = idle->image.stack;

    idle->started = 1;
    idle->running = 1;
    idle->wait_queue = list_create();
    idle->shm_mappings = list_create();
    idle->signal_queue = list_create();

    gettimeofday(&idle->start, NULL);

    set_process_environment(idle, current_directory);
    return idle;
}

static pid_t get_next_pid( void )
{
    if( _next_pid > MAX_PID )
    {
        int index = bitset_ffub(&pid_set);
        bitset_set(&pid_set, index);
        return index;
    }
    int pid = _next_pid;
    _next_pid++;
    
    bitset_set(&pid_set, pid);

    return pid;
}

process_t* spawn_process( volatile process_t* parent, int reuse_fds )
{
    process_t* proc = malloc(sizeof(process_t));
    memset(proc, 0, sizeof(process_t));
    proc->id = get_next_pid();
    proc->group = proc->id;
    proc->name = strdup(parent->name);
    proc->description = NULL;
    proc->cmdline = parent->cmdline;

    proc->user = parent->user;
    proc->real_user = parent->real_user;
    proc->user_group = parent->user_group;
    proc->mask = parent->mask;

    proc->job = parent->job;
    proc->session = parent->session;

    proc->thread.esp = 0;
    proc->thread.ebp = 0;
    proc->thread.eip = 0;
    proc->thread.fpu_enabled = 0;
    memcpy((void*)proc->thread.fp_regs, (void*)parent->thread.fp_regs, 512);

    proc->image.entry = parent->image.entry;
    proc->image.heap = parent->image.heap;
    proc->image.heap_actual = parent->image.heap_actual;
    proc->image.size = parent->image.size;

    proc->image.stack = (uintptr_t)kvmalloc(KERNEL_STACK_SIZE) + KERNEL_STACK_SIZE;
    proc->image.user_stack = parent->image.user_stack;
    proc->image.shm_heap = SHM_START;

    spin_init(proc->image.lock);

    if( reuse_fds )
    {
        proc->fds = parent->fds;
        proc->fds->refs++;
    }else
    {
        proc->fds = malloc(sizeof(fd_table_t));
        proc->fds->refs = 1;
        proc->fds->length = parent->fds->length;
        proc->fds->capacity = parent->fds->capacity;

        proc->fds->entries = malloc(proc->fds->capacity * sizeof(fs_node_t*));
        proc->fds->flags = malloc(proc->fds->capacity * sizeof(int));
        proc->fds->offsets = malloc(proc->fds->capacity * sizeof(uint64_t));

        for( uint32_t i = 0; i < parent->fds->length; i++ )
        {
            proc->fds->entries[i] = clone_fs(parent->fds->entries[i]);
            proc->fds->flags[i] = parent->fds->flags[i];
            proc->fds->offsets[i] = parent->fds->offsets[i];
        }
    }

    proc->wd_node = clone_fs(parent->wd_node);
    proc->wd_name = strdup(parent->wd_name);

    proc->status = 0;
    proc->finished = 0;
    proc->suspended = 0;
    proc->started = 0;
    proc->running = 0;
    memset(proc->signals.functions, 0x00, sizeof(uintptr_t) * NUMSIGNALS);
    proc->wait_queue = list_create();
    proc->shm_mappings = list_create();
    proc->signal_queue = list_create();
    proc->signal_kstack = NULL;

    proc->sched_node.prev = NULL;
    proc->sched_node.next = NULL;
    proc->sched_node.value = proc;

    proc->sleep_node.prev = NULL;
    proc->sleep_node.next = NULL;
    proc->sleep_node.value = proc;

    proc->timed_sleep_node = NULL;

    proc->is_daemon = 0;

    gettimeofday(&proc->start, NULL);

    tree_node_t* entry = tree_node_create(proc);
    proc->tree_entry = entry;
    
    spin_lock(tree_lock);
    tree_node_insert_child_node(process_tree, parent->tree_entry, entry);
    list_insert(process_list, (void*)proc);
    spin_unlock(tree_lock);

    return proc;
}

static uint8_t process_compare( void* proc_v, void* pid_v )
{
    pid_t pid = (*(pid_t*)pid_v);
    process_t* proc = (process_t*)proc_v;

    return (uint8_t)(proc->id == pid);
}

process_t* process_from_pid( pid_t pid )
{
    if( pid < 0 ) return NULL;

    spin_lock(tree_lock);
    tree_node_t* entry = tree_find(process_tree, &pid, process_compare);
    spin_unlock(tree_lock);
    if( entry )
    {
        return (process_t*)entry->value;
    }

    return NULL;
}

process_t* process_get_parent( process_t* process )
{
    process_t* result = NULL;

    spin_lock(tree_lock);
    tree_node_t* entry = process->tree_entry;

    if( entry->parent )
    {
        result = entry->parent->value;
    }
    spin_unlock(tree_lock);

    return result;
}

void set_process_environment( process_t* proc, page_directory_t* directory )
{
    proc->thread.page_directory = directory;
}

uint8_t process_available( void )
{
    return (process_queue->head != NULL);
}

uint32_t process_append_fd( process_t* proc, fs_node_t* node )
{
    for( unsigned int i = 0; i < proc->fds->length; i++ )
    {
        if( !proc->fds->entries[i] )
        {
            proc->fds->entries[i] = node;
            proc->fds->flags[i] = 0;
            proc->fds->offsets[i] = 0;
            return i;
        }
    }
    if( proc->fds->length == proc->fds->capacity )
    {
        proc->fds->capacity *= 2;
        proc->fds->entries = realloc(proc->fds->entries, proc->fds->capacity * sizeof(fs_node_t*));
        proc->fds->flags = realloc(proc->fds->flags, proc->fds->capacity * sizeof(int));
        proc->fds->offsets = realloc(proc->fds->offsets, proc->fds->capacity * sizeof(uint64_t));
    }
    proc->fds->entries[proc->fds->length] = node;
    proc->fds->flags[proc->fds->length] = 0;
    proc->fds->offsets[proc->fds->length] = 0;
    proc->fds->length++;

    return proc->fds->length-1;
}

uint32_t process_move_fd( process_t* proc, int src, int dest )
{
    if( (size_t)src >= proc->fds->length || (dest != -1 && (size_t)dest >= proc->fds->length) )
    {
        return -1;
    }
    if( dest == -1 )
    {
        dest = process_append_fd(proc, NULL);
    }
    if( proc->fds->entries[dest] )
    {
        close_fs(proc->fds->entries[dest]);
        proc->fds->entries[dest] = proc->fds->entries[src];
        proc->fds->flags[dest] = proc->fds->flags[src];
        proc->fds->offsets[dest] = proc->fds->offsets[src];
        open_fs(proc->fds->entries[dest], 0);
    }

    return dest;
}

int wakeup_queue( list_t* queue )
{
    char str[1024];
    int awoken_processes = 0;
    while( queue->length > 0 )
    {
        spin_lock(wait_lock_temp);
        node_t* node = list_pop(queue);
        spin_unlock(wait_lock_temp);
        if( !((process_t*)node->value)->finished )
        {
            make_process_ready(node->value);
            debug_logf(str, "%d - %s", ((process_t*)node->value)->id, ((process_t*)node->value)->name);
        }
        awoken_processes++;
    }

    return awoken_processes;
}

int wakeup_queue_interrupted( list_t* queue )
{
    int awoken_processes = 0;
    while( queue->length > 0 )
    {
        spin_lock(wait_lock_temp);
        node_t* node = list_pop(queue);
        spin_unlock(wait_lock_temp);
        if( !((process_t*)node->value)->finished )
        {
            process_t* proc = node->value;
            proc->sleep_interrupted = 1;
            make_process_ready(proc);
        }
        awoken_processes++;
    }

    return awoken_processes;
}

int sleep_on( list_t* queue )
{
    if( current_process->sleep_node.owner )
    {
        switch_task(0);
        return 0;
    }

    current_process->sleep_interrupted = 0;
    spin_lock(wait_lock_temp);
    list_append(queue, (node_t*)&current_process->sleep_node);
    spin_unlock(wait_lock_temp);
    switch_task(0);

    return current_process->sleep_interrupted;
}

int process_is_ready( process_t* proc )
{
    return (proc->sched_node.owner != NULL);
}

void wakeup_sleepers( unsigned long seconds, unsigned long subseconds )
{
    int_disable();

    spin_lock(sleep_lock);
    if( sleep_queue->length )
    {
        sleeper_t* proc = ((sleeper_t*)sleep_queue->head->value);
        while( proc && (proc->end_tick < seconds || (proc->end_tick == seconds && proc->end_subtick <= subseconds)) )
        {
            if( proc->is_fswait )
            {
                proc->is_fswait = -1;
                process_alert_node(proc->process, proc);
            }else
            {
                process_t* process = proc->process;
                process->sleep_node.owner = NULL;
                process->timed_sleep_node = NULL;
                if( !process_is_ready(process) )
                {
                    make_process_ready(process);
                }
            }
            free(proc);
            free(list_dequeue(sleep_queue));
            if( sleep_queue->length )
            {
                proc = ((sleeper_t*)sleep_queue->head->value);
            }else
            {
                break;
            }
        }
    }
    spin_unlock(sleep_lock);
    int_resume();
}

void sleep_until( process_t* process, unsigned long seconds, unsigned long subseconds )
{
    if( current_process->sleep_node.owner )
    {
        return;
    }
    process->sleep_node.owner = sleep_queue;

    int_disable();
    spin_lock(sleep_lock);
    node_t* before = NULL;
    foreach(node, sleep_queue)
    {
        sleeper_t* candidate = ((sleeper_t*)node->value);
        if( candidate->end_tick > seconds || (candidate->end_tick == seconds && candidate->end_subtick > subseconds) )
        {
            break;
        }
        before = node;
    }
    sleeper_t* proc = malloc(sizeof(sleeper_t));
    proc->process = process;
    proc->end_tick = seconds;
    proc->end_subtick = subseconds;
    proc->is_fswait = 0;
    process->timed_sleep_node = list_insert_after(sleep_queue, before, proc);
    spin_unlock(sleep_lock);

    int_resume();
}

void cleanup_process( process_t* proc, int retval )
{
    proc->status = retval;
    proc->finished = 1;

    list_free(proc->wait_queue);
    free(proc->wait_queue);
    list_free(proc->signal_queue);
    free(proc->signal_queue);
    free(proc->wd_name);

    if( proc->node_waits )
    {
        list_free(proc->node_waits);
        free(proc->node_waits);
        proc->node_waits = NULL;
    }
    
    shm_release_all(proc);
    free(proc->shm_mappings);

    if( proc->signal_kstack )
    {
        free(proc->signal_kstack);
    }

    release_directory(proc->thread.page_directory);

    proc->fds->refs--;
    if( proc->fds->refs == 0 )
    {
        for( uint32_t i = 0; i < proc->fds->length; i++ )
        {
            if( proc->fds->entries[i] )
            {
                close_fs(proc->fds->entries[i]);
                proc->fds->entries[i] = NULL;
            }
        }
        free(proc->fds->entries);
        free(proc->fds->flags);
        free(proc->fds->offsets);
        free(proc->fds);
        free((void*)(proc->image.stack - KERNEL_STACK_SIZE));
    }
}

void reap_process( process_t* proc )
{
    free(proc->name);
    delete_process(proc);
}

static int wait_candidate( process_t* parent, int pid, int options, process_t* proc )
{
    if( !proc ) return 0;
    if( options & WNOKERN )
    {
        if( proc->is_daemon ) return 0;
    }

    if( pid < -1 )
    {
        if( proc->job == -pid || proc->id == -pid ) return 1;
    }else if( pid == 0 )
    {
        if( proc->job == parent->id ) return 1;
    }else if( pid > 0 )
    {
        if( proc->id == pid ) return 1;
    }else
    {
        return 1;
    }
    
    return 0;
}

int waitpid( int pid, int* status, int options )
{
    process_t* proc = (process_t*)current_process;
    if( proc->group )
    {
        proc = process_from_pid(proc->group);
    }

    do{
        process_t* candidate = NULL;
        int has_children = 0;

        foreach(node, proc->tree_entry->children)
        {
            if( !node->value )
            {
                continue;
            }
            process_t* child = ((tree_node_t*)node->value)->value;

            if( wait_candidate(proc, pid, options, child) )
            {
                has_children = 1;
                if( child->finished )
                {
                    candidate = child;
                    break;
                }
                if( (options & WSTOPPED) && child->suspended )
                {
                    candidate = child;
                    break;
                }
            }
        }

        if( !has_children )
        {
            return -ECHILD;
        }

        if( candidate )
        {
            if( status )
            {
                *status = candidate->status;
            }
            int cpid = candidate->id;
            if( candidate->finished )
            {
                reap_process(candidate);
            }
            return cpid;
        }else
        {
            if( options & WNOHANG )
            {
                return 0;
            }
            if( sleep_on(proc->wait_queue) != 0 )
            {
                return -EINTR;
            }
        }
    } while(1);
}

int process_wait_nodes( process_t* process, fs_node_t* nodes[], int timeout )
{
    fs_node_t** n = nodes;
    int index = 0;
    if( *n )
    {
        do{
            int result = selectcheck_fs(*n);
            if( result < 0 )
            {
                return -1;
            }
            if( result == 0 )
            {
                return index;
            }
            n++;
            index++;
        } while( *n );
    }

    if( timeout == 0 )
    {
        return -2;
    }

    n = nodes;

    process->node_waits = list_create();
    if( *n )
    {
        do{
            if( selectwait_fs(*n, process) < 0 )
            {
                debug_log("bad selectwait?");
            }
            n++;
        }while(*n);
    }

    if( timeout > 0 )
    {
        unsigned long s, ss;
        relative_time(0, timeout, &s, &ss);

        int_disable();
        spin_lock(sleep_lock);
        node_t* before = NULL;
        foreach(node, sleep_queue)
        {
            sleeper_t* candidate = ((sleeper_t*)node->value);
            if( candidate->end_tick > s || (candidate->end_tick == s && candidate->end_subtick > ss) )
            {
                break;
            }
            before = node;
        }
        sleeper_t* proc = malloc(sizeof(sleeper_t));
        proc->process = process;
        proc->end_tick = s;
        proc->end_subtick = ss;
        proc->is_fswait = 1;
        list_insert(((process_t*)process)->node_waits, proc);
        process->timeout_node = list_insert_after(sleep_queue, before, proc);
        spin_unlock(sleep_lock);
        int_resume();
    }else
    {
        process->timeout_node = NULL;
    }

    process->awoken_index = -1;

    switch_task(0);

    return process->awoken_index;
}

int process_awaken_from_fswait( process_t* process, int index )
{
    process->awoken_index = index;
    list_free(process->node_waits);
    free(process->node_waits);
    process->node_waits = NULL;
    if( process->timeout_node && process->timeout_node->owner == sleep_queue )
    {
        sleeper_t* proc = process->timeout_node->value;
        if( proc->is_fswait != -1 )
        {
            list_delete(sleep_queue, process->timeout_node);
            free(process->timeout_node->value);
            free(process->timeout_node);
        }
    }
    process->timeout_node = NULL;
    make_process_ready(process);

    return 0;
}

int process_alert_node( process_t* process, void* value )
{
    if( !is_valid_process(process) )
    {
        return 0;
    }

    if( !process->node_waits )
    {
        return 0;
    }

    int index = 0;
    foreach(node, process->node_waits)
    {
        if( value == node->value )
        {
            return process_awaken_from_fswait(process, index);
        }
        index++;
    }

    return -1;
}

static void debug_print_proc_tree_node( char** str, tree_node_t* node, size_t height )
{
    /* End recursion on a blank entry */
	if (!node) return;

    char* tmp = calloc(512, sizeof(char));
	char* c = tmp;

    /* Indent output */
	for (uint32_t i = 0; i < height; ++i) {
		c += sprintf(c, "  ");
	}

    /* Get the current process */
	process_t* proc = (process_t*)node->value;

    /* Print the process name */
	if (proc) {
		c += sprintf(c, "%s - %d\n", proc->name, proc->id);
	}

    /* Linefeed */
    strcat(*str, tmp);
	free(tmp);

    foreach(child, node->children)
    {
		/* Recursively print the children */
		debug_print_proc_tree_node(str, child->value, height + 1);
    }
}

void debug_print_proc_tree( char** str )
{
	debug_print_proc_tree_node(str, process_tree->root, 0);
}
