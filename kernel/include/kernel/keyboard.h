#ifndef _KERNEL_KEYBOARD_H
#define _KERNEL_KEYBOARD_H

#include <sys/types.h>

#define KEY_DEVICE 0x60
#define KEY_PENDING 0x64
#define KEY_IRQ 1

int keyboard_install( void );
char convert_scancode( unsigned char );

#endif
