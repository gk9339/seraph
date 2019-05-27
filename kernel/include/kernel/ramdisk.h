#ifndef _KERNEL_RAMDISK_H
#define _KERNEL_RAMDISK_H

#include <kernel/fs.h>
#include <stddef.h>
#include <sys/types.h>

fs_node_t* ramdisk_mount( uintptr_t, size_t );

#endif
