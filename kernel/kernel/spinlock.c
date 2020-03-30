#include <kernel/spinlock.h>
#include <kernel/kernel.h>
#include <kernel/task.h>

// Atomic operation to swap *x and v
static inline int arch_atomic_swap( volatile int* x, int v )
{
    asm(
        "xchg %0, %1":"=r"(v), "=m"(*x):"0"(v):"memory"
        );
    return v;
}

// Atomic operation to store the value of x in *p
static inline void arch_atomic_store( volatile int* p, int x )
{
    asm(
        "movl %1, %0":"=m"(*p):"r"(x):"memory"
       );
}

// Atomic operation to increment *x
static inline void arch_atomic_inc( volatile int* x )
{
    asm(
        "lock; incl %0":"=m"(*x):"m"(*x):"memory"
       );
}

// Atomic operation to decrement *x
static inline void arch_atomic_dec( volatile int* x )
{
    asm(
        "lock; decl %0":"=m"(*x):"m"(*x):"memory"
       );
}

// Switch task while waiting for lock
static void spin_wait( volatile int* addr, volatile int* waiters )
{
    if( waiters )
    {
        arch_atomic_inc(waiters);
    }
    while( *addr )
    {
        switch_task(1);
    }
    if( waiters )
    {
        arch_atomic_dec(waiters);
    }
}

// Initialize spinlock
void spin_init( spin_lock_t lock )
{
    lock[0] = 0;
    lock[1] = 0;
}

// Lock spinlock
void spin_lock( spin_lock_t lock )
{
    while( arch_atomic_swap(lock, 1) )
    {
        spin_wait(lock, lock+1);
    }
}

// Unlock spinlock
void spin_unlock( spin_lock_t lock )
{
    if( lock[0] )
    {
        arch_atomic_store(lock, 0);
        if( lock[1] )
        {
            switch_task(1);
        }
    }
}
