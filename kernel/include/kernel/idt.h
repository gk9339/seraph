#ifndef _KERNEL_IDT_H
#define _KERNEL_IDT_H

#include <stdint.h> // intN_t

void idt_initialize( void );
void idt_set_gate( uint8_t num, void(*base)(void), uint16_t sel, uint8_t flags );

#endif
