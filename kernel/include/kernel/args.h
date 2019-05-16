#ifndef _KERNEL_ARGS_H
#define _KERNEL_ARGS_H

extern char* cmdline;

int args_present( char* karg );
char* args_value( char* karg );
void args_parse( char* _arg );

void early_stage_args( void );
void late_stage_args( void );

#endif
