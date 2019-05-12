#ifndef _KERNEL_PROCESS_H
#define _KERNEL_PROCESS_H

#include <kernel/types.h>
#include <kernel/mem.h>

#define KERNEL_STACK_SIZE 0x8000

typedef signed int pid_t;
typedef unsigned int user_t;
typedef unsigned int status_t;

#define USER_ROOT_UID (user_t)0

enum wait_option
{
    WCONTINUED,
    WNOHANG,
    WUNTRACED
};

typedef struct thread
{
    uintptr_t esp; /* Stack pointer */
    uintptr_t ebp; /* Base pointer */
    uintptr_t eip; /* Instruction pointer */

    uint8_t fpu_enabled;
    uint8_t fp_regs[512];

    uint8_t padding[32];

    page_directory_t* page_directory;
} thread_t;

typedef struct image
{
    size_t size;
    uintptr_t entry;
    uintptr_t heap;
    uintptr_t heap_actual;
    uintptr_t stack;
    uintptr_t user_stack;
    uintptr_t start;
    uintptr_t shm_heap;
    volatile int lock[2];
} image_t;

typedef struct process
{
    pid_t id;
    char* name;
    char* description;
    user_t user;
    int mask;
} process_t;

extern volatile process_t* current_process;

#endif
