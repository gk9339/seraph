#ifndef _KERNEL_TIMER_H
#define _KERNEL_TIMER_H

void timer_initialize( void );
extern unsigned long timer_ticks;
extern unsigned long timer_subticks;
extern signed long timer_drift;
void relative_time( unsigned long seconds, unsigned long subseconds, unsigned long* out_seconds, unsigned long* out_subseconds );

#endif
