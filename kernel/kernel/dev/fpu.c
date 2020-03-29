#include <kernel/fpu.h>
#include <kernel/serial.h>
#include <kernel/process.h>
#include <kernel/kernel.h>

process_t* fpu_thread = NULL;

uint8_t saves[512] __attribute__((aligned(16)));

static void enable_fpu( void )
{
    asm volatile (
                "clts"
                );
	
    size_t t;

    asm volatile (
                "mov %%cr0, %0" : "=r"(t)
                );
	
    t &= ~(1 << 2);
	t |= (1 << 1);
    asm volatile (
                "mov %0, %%cr0" :: "r"(t)
                );
	asm volatile (
                "mov %%cr4, %0" : "=r"(t)
                );

	t |= 3 << 9;
    asm volatile (
                "mov %0, %%cr4" :: "r"(t)
                );
}

static void restore_fpu( process_t* proc )
{
    memcpy(&saves, (uint8_t*)&proc->thread.fp_regs, 512);
    asm volatile(
            "fxrstor (%0)"
            ::"r"(saves)
            );
}

static void save_fpu( process_t* proc )
{
    asm volatile(
            "fxsave (%0)"
            ::"r"(saves)
            );
    memcpy((uint8_t*)&proc->thread.fp_regs, &saves, 512);
}

static void init_fpu( void )
{
    asm volatile("fninit");
}

void switch_fpu( void )
{
    save_fpu((process_t*)current_process);
}

void unswitch_fpu( void )
{
    restore_fpu((process_t*)current_process);
}

void fpu_initialize( void )
{
    enable_fpu();
    init_fpu();
    save_fpu((void*)current_process);
}
