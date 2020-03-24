#include <stdlib.h>
#include <string.h>
#include <stdio.h>

#include <kernel/task.h>
#include <kernel/kernel.h>
#include <kernel/process.h>
#include <kernel/signal.h>
#include <kernel/serial.h>
#include <kernel/mem.h>
#include <kernel/gdt.h>
#include <kernel/irq.h>
#include <sys/types.h>
#include <sys/signals.h>
#include <kernel/fpu.h>

#define TASK_MAGIC 0xcd014b19

uint32_t next_pid = 0;

#define PUSH(stack, type, item) stack -= sizeof(type); \
                                *((type*)stack) = item

page_directory_t* kernel_directory;
page_directory_t* current_directory;

extern char* default_name;
uintptr_t frozen_stack = 0;

void tasking_initialize( void )
{
    int_disable();

    initialize_process_tree();
    current_process = spawn_init();
    kernel_idle_task = spawn_kidle();

    switch_page_directory(current_process->thread.page_directory);

    frozen_stack = (uintptr_t)valloc(KERNEL_STACK_SIZE);

    int_resume();
}

uint32_t fork( void )
{
    int_disable();

    uintptr_t esp, ebp;

    current_process->syscall_registers->eax = 0;

    process_t* parent = (process_t*)current_process;

    page_directory_t* directory = clone_directory(current_directory);

    process_t* new_proc = spawn_process(current_process, 0);
    set_process_environment(new_proc, directory);

    struct regs r;
    memcpy(&r, current_process->syscall_registers, sizeof(struct regs));
    new_proc->syscall_registers = &r;

    esp = new_proc->image.stack;
    ebp = esp;

    new_proc->syscall_registers->eax = 0;

    PUSH(esp, struct regs, r);

    new_proc->thread.esp = esp;
    new_proc->thread.ebp = ebp;

    new_proc->is_daemon = parent->is_daemon;

    new_proc->thread.eip = (uintptr_t)&return_to_userspace;

    make_process_ready(new_proc);

    char debug_str[512];
    debug_logf(debug_str, "%d - %s -> Fork (%d)", new_proc->id, new_proc->name, current_process->id);
    
    int_resume();

    return new_proc->id;
}

int create_kernel_daemon( daemon_t daemon, char* name, void* argp )
{
    uintptr_t esp, ebp;

    int_disable();

    if( current_process->syscall_registers )
    {
        current_process->syscall_registers->eax = 0;
    }

    page_directory_t* directory = kernel_directory;
    process_t* new_proc = spawn_process(current_process, 0);
    set_process_environment(new_proc, directory);
    directory->ref_count++;

    if( current_process->syscall_registers )
    {
        struct regs r;
        memcpy(&r, current_process->syscall_registers, sizeof(struct regs));
        new_proc->syscall_registers = &r;
    }

    esp = new_proc->image.stack;
    ebp = esp;

    if( current_process->syscall_registers )
    {
        new_proc->syscall_registers->eax = 0;
    }
    new_proc->is_daemon = 1;
    new_proc->name = name;

    PUSH(esp, uintptr_t, (uintptr_t)name);
    PUSH(esp, uintptr_t, (uintptr_t)argp);
    PUSH(esp, uintptr_t, (uintptr_t)&task_exit);

    new_proc->thread.esp = esp;
    new_proc->thread.ebp = ebp;

    new_proc->thread.eip = (uintptr_t)daemon;

    make_process_ready(new_proc);

    int_resume();

    return new_proc->id;
}
    
uint32_t clone( uintptr_t new_stack, uintptr_t thread_func, uintptr_t arg )
{
    uintptr_t esp, ebp;

    int_disable();

    current_process->syscall_registers->eax = 0;

    process_t* parent = (process_t*)current_process;
    page_directory_t* directory = current_directory;

    process_t* new_proc = spawn_process(current_process, 1);
    set_process_environment(new_proc, directory);
    directory->ref_count++;

    struct regs r;
    memcpy(&r, current_process->syscall_registers, sizeof(struct regs));
    new_proc->syscall_registers = &r;

    esp = new_proc->image.stack;
    ebp = esp;

    if( current_process->group )
    {
        new_proc->group = current_process->group;
    }else
    {
        new_proc->group = current_process->id;
    }

    new_proc->syscall_registers->ebp = new_stack;
    new_proc->syscall_registers->eip = thread_func;

    PUSH(new_stack, uintptr_t, arg);
    PUSH(new_stack, uintptr_t, THREAD_RETURN);

    new_proc->syscall_registers->esp = new_stack;
    new_proc->syscall_registers->useresp = new_stack;

    PUSH(esp, struct regs, r);

    new_proc->thread.esp = esp;
    new_proc->thread.ebp = ebp;

    new_proc->is_daemon = parent->is_daemon;

    new_proc->thread.eip = (uintptr_t)&return_to_userspace;

    make_process_ready(new_proc);

    int_resume();

    return new_proc->id;
}

uint32_t getpid( void )
{
    return current_process->id;
}

void switch_task( uint8_t reschedule )
{
    if( !current_process )
    {
        return;
    }
    if( !current_process->running )
    {
        switch_next();
    }

    uintptr_t esp, ebp, eip;
    asm volatile( "mov %% esp, %0":"=r"(esp) );
    asm volatile( "mov %% ebp, %0":"=r"(ebp) );
    eip = read_eip();
    if( eip == 0x10000 )
    {
        fix_signal_stacks();

        if( !current_process->finished )
        {
            if( current_process->signal_queue->length > 0 )
            {
                node_t* node = list_dequeue(current_process->signal_queue);
                signal_t* sig = node->value;
                free(node);
                handle_signal((process_t*)current_process, sig);
            }
        }
        return;
    }

    current_process->thread.eip = eip;
    current_process->thread.esp = esp;
    current_process->thread.ebp = ebp;
    current_process->running = 0;

    switch_fpu();

    if( reschedule && current_process != kernel_idle_task )
    {
        make_process_ready((process_t*)current_process);
    }

    switch_next();
}

void switch_next( void )
{
    uintptr_t esp, ebp, eip;
    char debug[128];

    current_process = next_ready_process();

    eip = current_process->thread.eip;
    esp = current_process->thread.esp;
    ebp = current_process->thread.ebp;
    unswitch_fpu();

    if( (eip < (uintptr_t)&_kernel_start) || (eip > (uintptr_t)heap_end) )
    {
        sprintf(debug, "Skipping broken process %d (eip = 0x%x <0x%x or >0x%x)", current_process->id, eip, &_kernel_start, &_kernel_end);
        debug_log(debug);
        switch_next();
    }

    if( current_process->finished )
    {
        sprintf(debug, "Tried to switch to process %d, claims it is already finished.", current_process->id);
        debug_log(debug);
        switch_next();
    }

    current_directory = current_process->thread.page_directory;
    switch_page_directory(current_directory);
    set_kernel_stack(current_process->image.stack);

    if( current_process->started )
    {
        if( !current_process->signal_kstack )
        {
            if( current_process->signal_queue->length > 0 )
            {
                current_process->signal_kstack = malloc(KERNEL_STACK_SIZE);
                current_process->signal_state.esp = current_process->thread.esp;
                current_process->signal_state.eip = current_process->thread.eip;
                current_process->signal_state.ebp = current_process->thread.ebp;
                memcpy(current_process->signal_kstack, (void*)(current_process->image.stack - KERNEL_STACK_SIZE), KERNEL_STACK_SIZE);
            }
        }
    }else
    {
        current_process->started = 1;
    }

    current_process->running = 1;

    asm volatile
        (
         "mov %0, %%ebx\n"
         "mov %1, %%esp\n"
         "mov %2, %%ebp\n"
         "mov %3, %%cr3\n"
         "mov $0x10000, %%eax\n"
         "jmp *%%ebx"
         ::"r"(eip), "r"(esp),"r"(ebp),"r"(current_directory->physical_address)
         :"%ebx", "%esp", "%eax"
        );
}

extern void enter_userspace( uintptr_t location, uintptr_t stack );

void enter_user_jump( uintptr_t location, int argc, char** argv, uintptr_t stack )
{
    char debug_str[512];
    debug_logf(debug_str, "%d - %s -> Starting", current_process->id, current_process->name);
    int_disable();
    set_kernel_stack(current_process->image.stack);

    PUSH(stack, uintptr_t, (uintptr_t)argv);
    PUSH(stack, int, argc);
    enter_userspace(location, stack);
}

void task_exit( int retval )
{
    char debug_str[512];
    if( __builtin_expect(current_process->id == 0, 0) )/* Probably not a good thing */
    {
        switch_next();
        return;
    }

    debug_logf(debug_str, "%d - %s -> Finishing [%d]", current_process->id, current_process->name, retval);
    cleanup_process((process_t*)current_process, retval);

    process_t* parent = process_get_parent((process_t*)current_process);

    if( parent && !parent->finished )
    {
        send_signal(parent->group, SIGCHLD, 1);
        wakeup_queue(parent->wait_queue);
    }

    switch_next();
}

void kexit( int retval )
{
    task_exit(retval);
    debug_log("Process returned from task_exit. Stopping.");
    STOP;
}
