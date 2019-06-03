#ifndef _KERNEL_ARGS_H
#define _KERNEL_ARGS_H

int args_present( char* karg );
char* args_value( char* karg );
void args_parse( char* _arg );

#endif
