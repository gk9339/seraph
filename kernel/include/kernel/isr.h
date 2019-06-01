#ifndef _KERNEL_ISR_H
#define _KERNEL_ISR_H

#include <stddef.h>
#include <kernel/types.h>

void isr_initialize( void );
void isr_install_handler( size_t isr, irq_handler_t );
void isr_uninstall_handler( size_t isr );

void fault_handler( struct regs* r );

#endif
