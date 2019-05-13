#ifndef _KERNEL_TYPES_H
#define _KERNEL_TYPES_H

struct regs
{
    unsigned int gs, fs, es, ds;
    unsigned int edi, esi, ebp, esp, ebx, edx, ecx, eax;
    unsigned int int_no, err_code;
    unsigned int eip, cs, eflags, useresp, ss;
};
typedef struct regs regs_t;

typedef void(*irq_handler_t)(struct regs*);
typedef int(*irq_handler_chain_t)(struct regs*);

#endif
