#include <stdlib.h>
#include <string.h>

#include <kernel/task.h>
#include <kernel/process.h>
#include <kernel/mem.h>
#include <kernel/irq.h>

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

void switch_task( uint8_t reschedule )
{
    if( !current_process )
    {
        return;
    }
    return;
}
