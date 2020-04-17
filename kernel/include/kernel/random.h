#ifndef _KERNEL_RANDOM_H
#define _KERNEL_RANDOM_H

#include <stdint.h>

uint32_t __attribute__((pure)) xorshift( void );
int random_initialize( void );

#endif
