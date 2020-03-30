#ifndef _KERNEL_CMOS_H
#define _KERNEL_CMOS_H

#include <stdint.h> // intN_t

extern uint32_t boot_time; // Set in timer_initialize (timer.c)

void get_date( uint16_t* month, uint16_t* day ); //Get day/month from CMOS data
uint32_t read_cmos( void ); // Get CMOS data, convert to UNIX time
uint32_t now( void ); // Current time in UNIX time

#endif
