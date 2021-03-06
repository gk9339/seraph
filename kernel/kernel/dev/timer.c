#include <stdlib.h>
#include <string.h>
#include <kernel/timer.h>
#include <kernel/serial.h>
#include <kernel/kernel.h>
#include <kernel/irq.h>
#include <kernel/cmos.h>
#include <kernel/task.h>
#include <kernel/process.h>
#include <kernel/kconfig.h>

#define PIT_A 0x40
#define PIT_B 0x41
#define PIT_C 0x42
#define PIT_CONTROL 0x43

#define PIT_MASK 0xFF
#define PIT_SCALE 1193180
#define PIT_SET 0x34

#define TIMER_IRQ 0

unsigned long timer_ticks = 0;
unsigned long timer_subticks = 0;
signed long timer_drift = 0;
signed long _timer_drift = 0;

static int behind = 0;

// Set timer interval to value in hertz
static void timer_phase( int hz )
{
    int divisor = PIT_SCALE / hz;
    outportb(PIT_CONTROL, PIT_SET);
    outportb(PIT_A, (unsigned char)divisor & PIT_MASK);
    outportb(PIT_A, (unsigned char)(divisor >> 8) & PIT_MASK);
}

// IRQ handler for timer (IRQ0), switch to next task
static int timer_handler( struct regs* r __attribute__((unused)) )
{
    if( ++timer_subticks == SUBTICKS_PER_TICK || (behind && ++timer_subticks == SUBTICKS_PER_TICK) )
    {
        timer_ticks++;
        timer_subticks = 0;
        if( (timer_ticks & RESYNC_TIME) == 0 )
        {
            uint32_t new_time = read_cmos();
            _timer_drift = new_time - boot_time - timer_ticks;
            if( _timer_drift > 0 ) behind = 1;
            else behind = 0;
        }
    }
    irq_ack(TIMER_IRQ);

    wakeup_sleepers(timer_ticks, timer_subticks);
    switch_task(1);
    return 1;
}

// Convert from relative time to absolute time
void relative_time( unsigned long seconds, unsigned long subseconds, unsigned long* out_seconds, unsigned long* out_subseconds )
{
    if( subseconds + timer_subticks > SUBTICKS_PER_TICK )
    {
        *out_seconds = timer_ticks + seconds + 1;
        *out_subseconds = (subseconds + timer_subticks) - SUBTICKS_PER_TICK;
    }else
    {
        *out_seconds = timer_ticks + seconds;
        *out_subseconds = timer_subticks + subseconds;
    }
}

// Initialize boot time, install time IRQ, setup timer subticks
void timer_initialize( void )
{
    boot_time = read_cmos();
    irq_install_handler(TIMER_IRQ, timer_handler, "PIT timer interrupt");
    timer_phase(SUBTICKS_PER_TICK);
}
