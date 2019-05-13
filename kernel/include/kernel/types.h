#ifndef _KERNEL_TYPES_H
#define _KERNEL_TYPES_H

typedef unsigned long long uint64_t;
typedef unsigned long uint32_t;
typedef unsigned short uint16_t;
typedef unsigned char uint8_t;

typedef signed long int32_t;
typedef signed char int8_t;
typedef unsigned long uintptr_t;

typedef int gid_t;
typedef int uid_t;
typedef int dev_t;
typedef int ino_t;
typedef int mode_t;
typedef int caddr_t;

typedef unsigned short nlink_t;

typedef long off_t;
typedef long time_t;

typedef unsigned long useconds_t;
typedef long suseconds_t;
typedef int pid_t;

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
