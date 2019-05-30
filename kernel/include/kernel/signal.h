#ifndef _KERNEL_SIGNAL_H
#define _KERNEL_SIGNAL_H

#include <kernel/kernel.h>
#include <sys/types.h>
#include <kernel/process.h>

typedef struct
{
    uint32_t signum;
    uintptr_t handler;
    regs_t registers_before;
} signal_t;

void enter_signal_handler( uintptr_t location, int signum, uintptr_t stack );
void handle_signal( process_t* proc, signal_t* signal );

int send_signal( pid_t proc, uint32_t signal, int force );
int group_end_signal( int group, uint32_t signal, int force_root );

void return_from_signal_handler( void );
void fix_signal_stacks( void ) ;

#endif
