#ifndef _PTHREAD_H
#define _PTHREAD_H

#include <stdint.h>

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

int pthread_join( pthread_t thread, void** retval );

#define PTHREAD_MUTEX_INITIALIZER 0

int pthread_mutex_lock( pthread_mutex_t* mutex );
int pthread_mutex_trylock( pthread_mutex_t* mutex );
int pthread_mutex_unlock( pthread_mutex_t* mutex );
int pthread_mutex_init( pthread_mutex_t* mutex, const pthread_mutexattr_t* attr );
int pthread_mutex_destroy( pthread_mutex_t* mutex );

int pthread_attr_init( pthread_attr_t* attr );
int pthread_attr_destroy( pthread_attr_t* attr );

#endif
