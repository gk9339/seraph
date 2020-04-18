#ifndef _SPINLOCK_H
#define _SPINLOCK_H

#include <sched.h>

typedef volatile int spin_lock_t[2];

static void spin_lock( int volatile* lock )
{
    while(__sync_lock_test_and_set(lock, 0x01))
    {
        sched_yield();
    }
}

static void spin_unlock( int volatile* lock )
{
    __sync_lock_release(lock);
}

#endif
