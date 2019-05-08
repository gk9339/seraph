#ifndef _KERNEL_H
#define _KERNEL_H

#include <kernel/types.h>

#define asm __asm__
#define volatile __volatile__

#define SYSCALL_VECTOR 0x7F

#define PAUSE { asm volatile ("hlt"); };

#define KPANIC(mesg, regs) kpanic(mesg, __FILE__, __LINE__, regs);
#define STOP while(1){PAUSE};

void kpanic(char* error_message, const char* file, int line, struct regs* regs);

#endif
