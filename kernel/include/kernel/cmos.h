#ifndef _KERNEL_CMOS_H
#define _KERNEL_CMOS_H

#include <time.h>
#include <sys/types.h>

void get_time( uint16_t* hours, uint16_t* minutes, uint16_t* seconds );
void get_date( uint16_t* month, uint16_t* day );
extern uint32_t boot_time;
extern uint32_t read_cmos( void );

uint32_t now( void );

#endif
