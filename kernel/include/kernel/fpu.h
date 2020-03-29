#ifndef _KERNEL_FPU_H
#define _KERNEL_FPU_H

void switch_fpu( void ); // Save FPU registers when switching away from process
void unswitch_fpu( void ); // Restore FPU registers when switching to process
void fpu_initialize( void ); // Enable / initialize FPU

#endif
