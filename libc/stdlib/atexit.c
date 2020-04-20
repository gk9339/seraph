#include <stdlib.h>
#include <stddef.h>

typedef struct
{
    void (*destructor)(void*);
    void* object_ptr;
    void* dso_handle;
} atexit_fn_t;

atexit_fn_t __atexit_fn[ATEXIT_MAX];
unsigned int __atexit_fn_count = 0;

int __cxa_atexit( void (*function)(void*), void* object_ptr, void* dso );
void __cxa_finalize( void* function );

int atexit( void (*function)( void ) )
{
    return __cxa_atexit((void (*)(void*))function, NULL, NULL);
}

int __cxa_atexit( void (*function)(void*), void* object_ptr, void* dso_handle )
{
    if( __atexit_fn_count >= ATEXIT_MAX )
    {
        return -1;
    }
    __atexit_fn[__atexit_fn_count].destructor = function;
    __atexit_fn[__atexit_fn_count].object_ptr = object_ptr;
    __atexit_fn[__atexit_fn_count].dso_handle = dso_handle;
    __atexit_fn_count++;

    return 0;
}

void __cxa_finalize( void* function )
{
    void (*fnptr)(void*) = (void (*)(void*))(uintptr_t)function;
    unsigned int i = __atexit_fn_count;
    if( !function )
    {
        while( i-- )
        {
            if( __atexit_fn[i].destructor )
            {
                (*__atexit_fn[i].destructor)(__atexit_fn[i].object_ptr);
            }
        }
    }else
    {
        while( i-- )
        {
            if( __atexit_fn[i].destructor == fnptr )
            {
                (*__atexit_fn[i].destructor)(__atexit_fn[i].object_ptr);
                __atexit_fn[i].destructor = NULL;
            }
        }
    }
}
