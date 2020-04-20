#ifndef _WAIT_H
#define _WAIT_H

#ifdef __cplusplus
extern "C" {
#endif

#include <sys/types.h>

//void* mmap( void* addr, size_t len, int prot, int flags, int fildes, off_t off );
int mmap( uintptr_t address, size_t size );

int setheap( uintptr_t );

#ifdef __cplusplus
}
#endif

#endif
