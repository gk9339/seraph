#ifndef _KERNEL_H
#define _KERNEL_H

#include <kernel/types.h>
#include <sys/types.h>

#define ASSUME(cond) __extension__({ if(!(cond)) { __builtin_unreachable(); } })

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

void kpanic(char* error_message, const char* file, int line, struct regs* regs);

#endif
