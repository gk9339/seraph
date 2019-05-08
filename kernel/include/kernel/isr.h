#ifndef _KERNEL_ISR_H
#define _KERNEL_ISR_H

#include <stddef.h>
#include <kernel/kernel.h>
#include <kernel/types.h>
#include <kernel/symtab.h>

void isr_initialize( void );
void isr_install_handler( size_t isr, irq_handler_t );
void isr_uninstall_handler( size_t isr );

#endif
