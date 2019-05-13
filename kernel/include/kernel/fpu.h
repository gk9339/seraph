#ifndef _KERNEL_FPU_H
#define _KERNEL_FPU_H

void switch_fpu( void );
void unswitch_fpu( void );
void fpu_initialize( void );

#endif
