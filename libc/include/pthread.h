#ifndef _PTHREAD_H
#define _PTHREAD_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stdint.h>
#include <sched.h>
#include <time.h>

typedef struct
{
    uint32_t id;
    char* stack;
    void* retval;
} pthread_t;
typedef unsigned int pthread_attr_t;

int pthread_create( pthread_t* thread, pthread_attr_t* attr, void* (*start_routine)(void*), void* arg );
void pthread_exit( void* value );
int pthread_kill( pthread_t thread, int sig );

int clone( uintptr_t new_stack, uintptr_t thread_func, void* args );
int gettid( void );

void pthread_cleanup_push( void (*routine)(void*), void* arg );
void pthread_cleanup_pop( int execute );

typedef int volatile pthread_mutex_t;
typedef int pthread_mutexattr_t;
typedef unsigned int pthread_key_t;
typedef int pthread_once_t;
typedef int pthread_cond_t;
typedef int pthread_condattr_t;

int pthread_join( pthread_t thread, void** retval );

#define PTHREAD_ONCE_INIT 0
#define PTHREAD_COND_INITIALIZER 0
#define PTHREAD_MUTEX_INITIALIZER 0

#define PTHREAD_MUTEX_DEFAULT 0
#define PTHREAD_MUTEX_NORMAL 0
#define PTHREAD_MUTEX_ERRORCHECK 1
#define PTHREAD_MUTEX_RECURSIVE 2

int pthread_mutex_lock( pthread_mutex_t* mutex );
int pthread_mutex_trylock( pthread_mutex_t* mutex );
int pthread_mutex_unlock( pthread_mutex_t* mutex );
int pthread_mutex_init( pthread_mutex_t* mutex, const pthread_mutexattr_t* attr );
int pthread_mutex_destroy( pthread_mutex_t* mutex );

int pthread_attr_init( pthread_attr_t* attr );
int pthread_attr_destroy( pthread_attr_t* attr );

int pthread_once( pthread_once_t* once_control, void (*init_routine)(void) );
void* pthread_getspecific( pthread_key_t key );
int pthread_setspecific( pthread_key_t key, const void* value );
int pthread_equal( pthread_t t1, pthread_t t2 );
pthread_t pthread_self( void );
int pthread_detach( pthread_t thread );
int pthread_cancel( pthread_t thread );
int pthread_cond_destroy( pthread_cond_t* cond );
int pthread_cond_init( pthread_cond_t* cond, const pthread_condattr_t* attr );
int pthread_cond_broadcast( pthread_cond_t* cond );
int pthread_cond_signal( pthread_cond_t* cond );
int pthread_cond_timedwait( pthread_cond_t* cond, pthread_mutex_t* mutex, const struct timespec* abstime);
int pthread_cond_wait( pthread_cond_t* cond, pthread_mutex_t* mutex);
int pthread_key_create( pthread_key_t* key, void (*destructor)(void*) );
int pthread_key_delete( pthread_key_t key );
int pthread_mutexattr_init( pthread_mutexattr_t* attr );
int pthread_mutexattr_destroy( pthread_mutexattr_t* attr );
int pthread_mutexattr_settype( pthread_mutex_t* attr, int type );
int pthread_mutex_timedlock( pthread_mutex_t* mutex, const struct timespec* abstime );

#ifdef __cplusplus
}
#endif

#endif
