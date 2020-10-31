#include <stdlib.h>
#include <dlfcn.h>

static char* error = "dlopen functions not available";

void* __attribute__((weak)) dlopen( const char* filename __attribute__((unused)) , int flags __attribute__((unused)) )
{
    return NULL;
}

int __attribute__((weak)) dlclose( void* handle __attribute__((unused)) )
{
    return -1;
}

void* dlsym( void* handle __attribute__((unused)), const char* symbol __attribute__((unused)) )
{
    return NULL;
}

char* dlerror( void )
{
    return error;
}
