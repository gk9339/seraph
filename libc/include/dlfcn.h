#ifndef _DLFCN_H
#define _DLFCN_H

#ifdef __cplusplus
extern "C" {
#endif

// Currently unused...
#define RTLD_LAZY   (1 << 0)
#define RTLD_NOW    (1 << 1)
#define RTLD_GLOBAL (1 << 2)

#define RTLD_DEFAULT ((void*)0)

// Provided by ld.so, but also defined by libc.so for linking
extern void* dlopen( const char*, int );
extern int dlclose( void* );
extern void* dlsym( void*, const char* );
extern char* dlerror( void );

#ifdef __cplusplus
}
#endif

#endif
