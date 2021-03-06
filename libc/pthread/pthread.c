#include <stdlib.h>
#include <stdint.h>
#include <sys/syscall.h>
#include <signal.h>
#include <pthread.h>
#include <errno.h>
#include <sched.h>
#include <sys/wait.h>

DEFN_SYSCALL3(clone, SYS_CLONE, uintptr_t, uintptr_t, void*)
DEFN_SYSCALL0(gettid, SYS_GETTID)

#define PTHREAD_STACK_SIZE 0x100000

int clone( uintptr_t new_stack, uintptr_t thread_func, void* arg )
{
    __sets_errno(syscall_clone(new_stack ,thread_func, arg));
}

int gettid( void )
{
    return syscall_gettid();
}

int pthread_create( pthread_t* thread, pthread_attr_t* attr __attribute__((unused)), void* (*start_routine)(void*), void* arg )
{
    char* stack = malloc(PTHREAD_STACK_SIZE);
    uintptr_t stack_top = (uintptr_t)stack + PTHREAD_STACK_SIZE;
    thread->stack = stack;
    thread->id = clone(stack_top, (uintptr_t)start_routine, arg);

    return 0;
}

int pthread_kill( pthread_t thread, int sig )
{
    __sets_errno(kill(thread.id, sig));
}

void pthread_exit( void* value __attribute__((unused)) ) //STUB
{
    exit(1);
}

void pthread_cleanup_push( void (*routine)(void*) __attribute__((unused)), void* arg __attribute__((unused)) )
{
    //STUB
}

void pthread_cleanup_pop( int execute __attribute__((unused)) )
{
    //STUB
}

int pthread_mutex_lock( pthread_mutex_t* mutex )
{
    while( __sync_lock_test_and_set(mutex, 1) )
    {
        sched_yield();
    }
    return 0;
}

int pthread_mutex_unlock( pthread_mutex_t* mutex )
{
    __sync_lock_release(mutex);
    return 0;
}

int pthread_mutex_init( pthread_mutex_t* mutex, const pthread_mutexattr_t* attr __attribute__((unused)) )
{
    *mutex = 0;
    return 0;
}

int pthread_mutex_destroy( pthread_mutex_t* mutex __attribute__((unused)) )
{
    return 0;
}

int pthread_attr_init( pthread_attr_t* attr )
{
    *attr = 0;
    return 0;
}

int pthread_attr_destroy( pthread_attr_t* attr __attribute__((unused)) )
{
    return 0;
}

int pthread_join( pthread_t thread, void** retval )
{
    int status;
    int result = waitpid(thread.id, &status, 0);
    if( retval )
    {
        *retval = (void*)status;
    }

    return result;
}

int pthread_mutexattr_init( pthread_mutexattr_t* attr )
{
    return 0;
}

int pthread_mutexattr_destroy( pthread_mutexattr_t* attr )
{
    return 0;
}

int pthread_mutexattr_settype( pthread_mutex_t* attr, int type )
{
    return 0;
}

int pthread_once( pthread_once_t* once_control, void (*init_routine)(void) )
{
    return 0;
}

int pthread_cond_wait( pthread_cond_t* cond, pthread_mutex_t* mutex )
{
    return 0;
}

int pthread_cond_broadcast( pthread_cond_t* cond )
{
    return 0;
}

int pthread_key_create( pthread_key_t* key, void (*destructor)(void*) )
{
    return 0;
}

int pthread_key_delete( pthread_key_t key )
{
    return 0;
}

void* pthread_getspecific( pthread_key_t key )
{
    return 0;
}

int pthread_setspecific( pthread_key_t key, const void* value )
{
    return 0;
}
