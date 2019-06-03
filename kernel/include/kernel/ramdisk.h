#ifndef _KERNEL_RAMDISK_H
#define _KERNEL_RAMDISK_H

#include <kernel/fs.h> /* fs_node_t */
#include <stddef.h> /* size_t */
#include <sys/types.h> /* uintptr_t */

fs_node_t* ramdisk_mount( uintptr_t, size_t );

#endif
