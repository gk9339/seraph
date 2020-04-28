#ifndef _KCONFIG_H
#define _KCONFIG_H

// Memory
#define KERNEL_STACK_SIZE 0x8000

#define KERNEL_HEAP_INIT 0x00800000
#define KERNEL_HEAP_END 0x20000000

#define USER_STACK_BOTTOM 0xAFF00000
#define USER_STACK_TOP 0xB0000000

// Shared memory
#define SHM_START 0xB0000000

// VFS
#define MAX_SYMLINK_DEPTH 8
#define MAX_SYMLINK_SIZE 4096

// Interrupt requests
#define IRQ_CHAIN_SIZE 16
#define IRQ_CHAIN_DEPTH 4

// Timing
#define SUBTICKS_PER_TICK 1000 // Hz
#define RESYNC_TIME 1

// Debug
#define EARLY_KERNEL_DEBUG 1 // Log to serial during early boot

// Process
#define MAX_PID 32768 // Makes a 4096-byte bitmap

// PTY
#define PTY_BUFFER_SIZE 8196

#endif
