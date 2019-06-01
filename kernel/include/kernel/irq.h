#ifndef _KERNEL_IRQ_H
#define _KERNEL_IRQ_H

#define SYSCALL_VECTOR 0x7F

#include <stddef.h>
#include <kernel/types.h>

void int_disable( void );
void int_resume( void );
void int_enable( void );

void irq_initialize( void );
void irq_install_handler( size_t irq, irq_handler_chain_t, char* desc );
void irq_uninstall_handler( size_t irq );
void irq_is_handler_free( size_t irq );
void irq_gates( void );
void irq_ack( size_t );

char* get_irq_handler( int irq, int chain );

void irq_handler( struct regs* r );

#endif
