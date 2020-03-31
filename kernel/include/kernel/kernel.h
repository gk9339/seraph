#ifndef _KERNEL_H
#define _KERNEL_H

#include <kernel/types.h> // struct regs
#include <stdint.h> // intN_t

#define ASSUME(cond) __extension__({ if(!(cond)) { __builtin_unreachable(); } })
#define CHECK_FLAG(flags,bit) ((flags)&(1<<((bit))))

#define STR(x) #x
#define STRSTR(x) STR(x)

#define asm __asm__
#define volatile __volatile__

#define PAUSE { asm volatile ("hlt"); };

#define KPANIC(mesg, regs) kpanic(mesg, __FILE__, __LINE__, regs);
#define STOP while(1){PAUSE};

extern void* _kernel_start;
extern void* _kernel_end;
extern uintptr_t initial_esp;
extern int debug;

void kernel_main( unsigned long magic, unsigned long addr, uintptr_t esp );
void kpanic(char* error_message, const char* file, int line, struct regs* regs);

#endif
