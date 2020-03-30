#ifndef _KERNEL_TIMER_H
#define _KERNEL_TIMER_H

extern unsigned long timer_ticks;
extern unsigned long timer_subticks;
extern signed long timer_drift;

void timer_initialize( void ); // Initialize boot time, install timer IRQ, setup timer subticks
void relative_time( unsigned long seconds, unsigned long subseconds, unsigned long* out_seconds, unsigned long* out_subseconds ); // Convert from relative time to absolute time

#endif
