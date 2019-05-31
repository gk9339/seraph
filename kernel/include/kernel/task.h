#ifndef _KERNEL_TASK_H
#define _KERNEL_TASK_H

#include <sys/types.h>

uintptr_t read_eip( void );
void kexit( int retval );
void task_exit( int retval );
extern uint32_t next_pid;

void tasking_initialize( void );
void switch_task( uint8_t reschedule );
void switch_next( void );
uint32_t fork( void );
uint32_t clone( uintptr_t new_stack, uintptr_t thread_func, uintptr_t arg );
uint32_t getpid( void );

void enter_user_jump( uintptr_t location, int argc, char** argv, uintptr_t stack );
extern void return_to_userspace( void );

#endif
