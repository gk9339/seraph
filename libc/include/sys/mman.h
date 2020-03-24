#ifndef _WAIT_H
#define _WAIT_H

#include <sys/types.h>

//void* mmap( void* addr, size_t len, int prot, int flags, int fildes, off_t off );
int mmap( uintptr_t address, size_t size );

int setheap( uintptr_t );

#endif
