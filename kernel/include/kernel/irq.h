#ifndef _KERNEL_IRQ_H
#define _KERNEL_IRQ_H

#include <stddef.h>
#include <stdio.h>
#include <kernel/types.h>
#include <kernel/kernel.h>
#include <kernel/idt.h>
#include <kernel/serial.h>

void int_disable( void );
void int_resume( void );
void int_enable( void );

void irq_initialize( void );
void irq_install_handler( size_t irq, irq_handler_chain_t, char* desc );
void irq_uininstall_handler( size_t irq );
void irq_is_handler_free( size_t irq );
void irq_gates( void );
void irq_ack( size_t );

#endif
