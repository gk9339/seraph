#include <stdlib.h>
#include <string.h>
#include <kernel/serial.h>
#include <kernel/signal.h>
#include <kernel/kernel.h>
#include <sys/types.h>
#include <kernel/spinlock.h>
#include <sys/signals.h>
#include <kernel/process.h>
#include <kernel/irq.h>
#include <errno.h>
#include <kernel/task.h>

static spin_lock_t sig_lock;
static spin_lock_t sig_lock_b;

list_t* rets_from_sig;

char isdeadly[] = {
    0, /* 0? */
    1, /* SIGHUP     */
    1, /* SIGINT     */
    2, /* SIGQUIT    */
    2, /* SIGILL     */
    2, /* SIGTRAP    */
    2, /* SIGABRT    */
    2, /* SIGEMT     */
    2, /* SIGFPE     */
    1, /* SIGKILL    */
    2, /* SIGBUS     */
    2, /* SIGSEGV    */
    2, /* SIGSYS     */
    1, /* SIGPIPE    */
    1, /* SIGALRM    */
    1, /* SIGTERM    */
    1, /* SIGUSR1    */
    1, /* SIGUSR2    */
    0, /* SIGCHLD    */
    0, /* SIGPWR     */
    0, /* SIGWINCH   */
    0, /* SIGURG     */
    0, /* SIGPOLL    */
    3, /* SIGSTOP    */
    3, /* SIGTSTP    */
    4, /* SIGCONT    */
    3, /* SIGTTIN    */
    3, /* SIGTTOUT   */
    1, /* SIGVTALRM  */
    1, /* SIGPROF    */
    2, /* SIGXCPU    */
    2, /* SIGXFSZ    */
    0, /* SIGWAITING */
    1, /* SIGDIAF    */
    0, /* SIGHATE    */
    0, /* SIGWINEVENT*/
    0, /* SIGCAT     */
};

void enter_signal_handler( uintptr_t location, int signum, uintptr_t stack )
{
    int_disable();
    uintptr_t ebp = current_process->syscall_registers->ebp;
    uintptr_t esp = current_process->syscall_registers->esp;
    asm volatile(
        "mov %2, %%esp\n"
        "pushl %4\n"
        "pushl %3\n"
        "pushl %2\n"
        "pushl $" STRSTR(SIGNAL_RETURN) "\n"
        "mov $0x23, %%ax\n"
        "mov %%ax, %%ds\n"
        "mov %%ax, %%es\n"
        "mov %%ax, %%fs\n"
        "mov %%ax, %%gs\n"
        "mov %%esp, %%eax\n"
        "pushl $0x23\n"
        "pushl %%eax\n"
        "pushf\n"
        "popl %%eax\n"
        "orl $0x200, %%eax\n"
        "pushl %%eax\n"
        "pushl $0x1B\n"
        "iret\n"
        ::"m"(location), "m"(signum), "r"(stack), "m"(ebp), "m"(esp): "%ax", "%esp", "%eax"
        );
}

void handle_signal( process_t* proc, signal_t* sig )
{
    uintptr_t handler = sig->handler;
    uintptr_t signum = sig->signum;
    free(sig);

    if( proc->finished )
    {
        return;
    }

    if( signum == 0 || signum >= NUMSIGNALS )
    {
        return;
    }

    if( !handler )
    {
        char dowhat = isdeadly[signum];
        if( dowhat == 1 || dowhat == 2 )
        {
            kexit(((128 + signum) << 8) | signum);
            __builtin_unreachable();
        }else if( dowhat == 3)
        {
            current_process->suspended = 1;
            current_process->status = 0x7F;

            process_t* parent = process_get_parent((process_t*)current_process);

            if( parent && !parent->finished )
            {
                wakeup_queue(parent->wait_queue);
            }

            switch_task(0);
        }else if( dowhat == 4 )
        {
            switch_task(1);
            return;
        }else
        {
            debug_log("Ignoring signal by default");
        }

        return;
    }

    if( handler == 1 )
    {
        return;
    }

    uintptr_t stack = 0xFFFF0000;
    if( proc->syscall_registers->useresp < 0x10000100 )
    {
        stack = proc->image.user_stack;
    }else
    {
        stack = proc->syscall_registers->useresp;
    }

    enter_signal_handler(handler, signum, stack);
}

void return_from_signal_handler( void )
{
    if( __builtin_expect(!rets_from_sig, 0))
    {
        rets_from_sig = list_create();
    }

    spin_lock(sig_lock);
    list_insert(rets_from_sig, (process_t*)current_process);
    spin_unlock(sig_lock);

    switch_next();
}

void fix_signal_stacks( void )
{
    uint8_t redo_me = 0;
    if( rets_from_sig )
    {
        spin_lock(sig_lock_b);
        while( rets_from_sig->head )
        {
            spin_lock(sig_lock);
            node_t* n = list_dequeue(rets_from_sig);
            spin_unlock(sig_lock);
            if( !n )
            {
                continue;
            }
            process_t* p = n->value;
            if( p == current_process )
            {
                redo_me = 1;
                continue;
            }
            p->thread.esp = p->signal_state.esp;
            p->thread.ebp = p->signal_state.ebp;
            p->thread.eip = p->signal_state.eip;
            if( !p->signal_kstack )
            {
                debug_log("Cannot restore signal stack");
            }else
            {
                debug_log("Restoring signal stack");
                memcpy((void*)(p->image.stack - KERNEL_STACK_SIZE), p->signal_kstack, KERNEL_STACK_SIZE);
                free(p->signal_kstack);
                p->signal_kstack = NULL;
            }
            make_process_ready(p);
        }
        spin_unlock(sig_lock_b);
    }
    if( redo_me )
    {
        spin_lock(sig_lock);
        list_insert(rets_from_sig, (process_t*)current_process);
        spin_unlock(sig_lock);
        switch_next();
    }
}

int send_signal( pid_t proc, uint32_t signal, int force_root )
{
    process_t* receiver = process_from_pid(proc);

    if( !receiver )
    {
        return -ESRCH;
    }

    if( !force_root && receiver->user != current_process->user && current_process->user != USER_ROOT_UID )
    {
        if( !(signal == SIGCONT && receiver->session == current_process->session ) )
        {
            return -EPERM;
        }
    }

    if( signal > NUMSIGNALS )
    {
        return -EINVAL;
    }

    if( receiver->finished )
    {
        return -EINVAL;
    }

    if( !receiver->signals.functions[signal] && !isdeadly[signal])
    {
        return 0;
    }

    if( isdeadly[signal] == 4 )
    {
        if( !receiver->suspended )
        {
            return -EINVAL;
        } else
        {
            receiver->suspended = 0;
            receiver->status = 0;
        }
    }

    signal_t* sig = malloc(sizeof(signal_t));
    sig->handler = (uintptr_t)receiver->signals.functions[signal];
    sig->signum = signal;
    memset(&sig->registers_before, 0x00, sizeof(regs_t));

    if( receiver->node_waits )
    {
        process_awaken_from_fswait(receiver, -1);
    }
    if( !process_is_ready(receiver) )
    {
        make_process_ready(receiver);
    }

    list_insert(receiver->signal_queue, sig);

    if( receiver == current_process )
    {
        if( receiver->signal_kstack )
        {
            switch_next();
        }else
        {
            switch_task(0);
        }
    }

    return 0;
}

int group_send_singal( int group, uint32_t signal, int force_root )
{
    int kill_self = 0;
    int killed_something = 0;

    foreach(node, process_list)
    {
        process_t* proc = node->value;
        if( proc->group == proc->id && proc->job == group )
        {
            if( proc->group == current_process->group )
            {
                kill_self = 1;
            }else
            {
                if( send_signal(proc->group, signal, force_root) == 0 )
                {
                    killed_something = 1;
                }
            }
        }
    }

    if( kill_self )
    {
        if( send_signal(current_process->group, signal, force_root) == 0 )
        {
            killed_something = 1;
        }
    }

    return !!killed_something;
}
