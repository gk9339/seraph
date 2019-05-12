#ifndef _KERNEL_H
#define _KERNEL_H

#include <kernel/types.h>

#define ASSUME(cond) __extension__({ if(!(cond)) { __builtin_unreachable(); } })

#define asm __asm__
#define volatile __volatile__

#define PAUSE { asm volatile ("hlt"); };

#define KPANIC(mesg, regs) kpanic(mesg, __FILE__, __LINE__, regs);
#define STOP while(1){PAUSE};

extern void* _kernel_start;
extern void* _kernel_end;

void kpanic(char* error_message, const char* file, int line, struct regs* regs);

#endif
