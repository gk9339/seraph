#ifndef _KERNEL_ISR_H
#define _KERNEL_ISR_H

#include <stddef.h> // size_t
#include <kernel/types.h> // irq_handler_t, struct regs

void isr_initialize( void );
void isr_install_handler( size_t isr, irq_handler_t );
void isr_uninstall_handler( size_t isr );

void fault_handler( struct regs* r );

#endif
