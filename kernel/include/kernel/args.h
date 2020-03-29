#ifndef _KERNEL_ARGS_H
#define _KERNEL_ARGS_H

int args_present( char* karg ); // Test if karg is present in kernel arguments
char* args_value( char* karg ); // Get value of karg from kernel arguments
void args_parse( char* cmdline ); // Parse cmdline as new kernel arguments

#endif
