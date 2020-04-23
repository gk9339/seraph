#ifndef _WAIT_H
#define _WAIT_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stdint.h>
#include <stddef.h>
#include <sys/types.h>

#define PROT_NONE  0x00
#define PROT_READ  0x01
#define PROT_WRITE 0x02
#define PROT_EXEC  0x04

#define MAP_PRIVATE   0x01
#define MAP_SHARED    0x02
#define MAP_FIXED     0x04
#define MAP_ANON      0x08
#define MAP_ANONYMOUS 0x08

void* mmap( void* addr, size_t len, int prot, int flags, int fildes, off_t off );
int munmap( void* addr, size_t len );

int lnk_mmap( uintptr_t address, size_t size );
int setheap( uintptr_t );

#ifdef __cplusplus
}
#endif

#endif
