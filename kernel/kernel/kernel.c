#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <kernel/kernel.h>
#include <kernel/multiboot.h>
#include <kernel/serial.h>
#include <kernel/gdt.h>
#include <kernel/idt.h>
#include <kernel/isr.h>
#include <kernel/irq.h>
#include <kernel/mem.h>
#include <kernel/tty.h>
#include <kernel/fs.h>
#include <kernel/task.h>
#include <kernel/process.h>
#include <kernel/timer.h>
#include <kernel/cmos.h>
#include <kernel/fpu.h>
#include <kernel/shm.h>
#include <kernel/keyboard.h>
#include <kernel/ext2.h>
#include <kernel/tarfs.h>
#include <sys/types.h>
#include <kernel/args.h>
#include <kernel/elf.h>

#define CHECK_FLAG(flags,bit) ((flags)&(1<<(bit)))

uintptr_t initial_esp = 0;

void kernel_main( unsigned long magic, unsigned long addr, uintptr_t esp ) 
{
    debug_log("Start kernel_main");
    char debug_str[256];
    multiboot_info_t* mbi;
    initial_esp = esp;
    extern char* cmdline;

    /* CPU Initialization */
    debug_log("GDT / IDT initialization");
    gdt_initialize();
    idt_initialize();

    /* Terminal Initialization */
    debug_log("Terminal initialization");
    terminal_initialize();

    printf("Booting...\n");

    /* Multiboot check */
    if( magic != MULTIBOOT_BOOTLOADER_MAGIC )
    {
        KPANIC("INVALID MAGIC NUMBER", NULL);
    }
    mbi = (multiboot_info_t*)addr;
    
    /* Memory Initialization */
    if( !CHECK_FLAG(mbi->flags, 0) ) /* mem_upper/mem_lower  */
    {
        KPANIC("Missing MEM flag in multiboot header", NULL)
    }
    paging_install(mbi->mem_lower + mbi->mem_upper);

    if( CHECK_FLAG(mbi->flags, 6) ) /* mmap */
    {
        debug_log("Parsing memory map.");
        multiboot_memory_map_t* mmap = (void*)mbi->mmap_addr;
        while( (uintptr_t)mmap < mbi->mmap_addr + mbi->mmap_length )
        {
            if(mmap->type == 2)
            {
                for( unsigned long int i = 0; i < mmap->len; i+= 0x1000 )
                {
                    if( mmap->addr + i > 0xFFFFFFFF ) break;
                    sprintf(debug_str, "Marking 0x%x", (uint32_t)mmap->addr + i);
                    debug_log(debug_str);
                    paging_mark_system((mmap->addr + i) & 0xFFFFF000);
                }
            }
            mmap = (multiboot_memory_map_t*)((uintptr_t)mmap + mmap->size + sizeof(uintptr_t));
        }
    }
    debug_log("Finalize paging / heap install");
    paging_finalize();
    
    char cmdline_[1024];
    if( CHECK_FLAG(mbi->flags, 2) ) /* cmdline */
    {
        size_t len = strlen((char*)mbi->cmdline);
        memmove(cmdline_, (char*)mbi->cmdline, len + 1);
    }

    heap_install();

    args_parse(cmdline_);

    /* Interrupts Initialization */
    debug_log("Interrupts initialization");
    isr_initialize();
    irq_initialize();
    
    debug_log("vfs initialization");
    vfs_initialize();

    debug_log("Tasking initialization");
    tasking_initialize();

    debug_log("Timer initialization");
    timer_initialize();

    debug_log("FPU initialization");
    fpu_initialize();

    debug_log("SHM initialization");
    shm_initialize();

    /* Test keyboard handler */
    debug_log("Install keyboard handler");
    keyboard_install();

    debug_log("Initialize fs types");
    tarfs_initialize();
    ext2_initialize();

    map_vfs_directory("/dev");
    
    debug_log("setup root mount");
    if( args_present("root") )
    {
        char* root_type = "ext2";
        if(args_present("root_type"))
        {
            root_type = args_value("root_type");
        }
        debug_log(root_type);
        vfs_mount_type(root_type, args_value("root"), "/");
    }else
    {
        KPANIC("No root option given", NULL)
    }

    char* argv[] =
    {
        "init"
    };
    int argc = 0;
    while( argv[argc] )
    {
        argc++;
    }

    system(argv[0], argc, argv, NULL);

    debug_log("INIT FAILED");
    KPANIC("INIT FAILED", NULL)
}

void kpanic( char* error_message, const char* file, int line, struct regs* regs )
{
    printf("PANIC: %s ", error_message);
    printf("File: %s ", file);
    printf("Line: %x ", line);
    if(regs)
    {
        printf("\nREGISTERS:");
        printf("eax=0x%x ebx=0x%x\n", regs->eax, regs->ebx);
        printf("ecx=0x%x edx=0x%x\n", regs->ecx, regs->edx);
        printf("esp=0x%x ebp=0x%x\n", regs->esp, regs->ebp);
        printf("ERRCD: 0x%x ", regs->err_code);
        printf("EFLAGS: 0x%x\n", regs->eflags);
        printf("User ESP: 0x%x ", regs->useresp);
        printf("eip=0x%x\n", regs->eip);
    }

    int_disable();
    STOP
}
