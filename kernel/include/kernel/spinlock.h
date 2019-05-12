#ifndef _KERNEL_SPINLOCK_H
#define _KERNEL_SPINLOCK_H

typedef volatile int spin_lock_t[2];

void spin_init( spin_lock_t lock );
void spin_lock( spin_lock_t lock );
void spin_unlock( spin_lock_t lock );

#endif
