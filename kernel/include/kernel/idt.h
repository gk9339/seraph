#ifndef _KERNEL_IDT_H
#define _KERNEL_IDT_H

#include <kernel/types.h>
#include <string.h>

void idt_initialize( void );
void idt_set_gate( uint8_t num, void(*base)(void), uint16_t sel, uint8_t flags );

#endif
