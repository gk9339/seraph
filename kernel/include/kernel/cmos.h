#ifndef _KERNEL_CMOS_H
#define _KERNEL_CMOS_H

#include <stdint.h> // intN_t

extern uint32_t boot_time; // Set in timer_initialize (timer.c)

void get_time( uint16_t* hours, uint16_t* minutes, uint16_t* seconds );
void get_date( uint16_t* month, uint16_t* day );
uint32_t read_cmos( void );
uint32_t now( void );

#endif
