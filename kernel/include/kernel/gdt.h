#ifndef _KERNEL_GDT_H
#define _KERNEL_GDT_H

#include <stdint.h>

void gdt_initialize( void );
void gdt_set_gate( uint8_t num, uint64_t base, uint64_t limit, uint8_t access, uint8_t gran );
void set_kernel_stack( uintptr_t stack );

#endif
