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
#include <kernel/ramdisk.h>
#include <kernel/syscall.h>

#define CHECK_FLAG(flags,bit) ((flags)&(1<<(bit)))

uintptr_t initial_esp = 0;

void kernel_main( unsigned long magic, unsigned long addr, uintptr_t esp ) 
{
    debug_log("Start kernel_main");
    char debug_str[256];
    uint32_t mboot_mods_count = 0;
    multiboot_info_t* mbi;
    multiboot_module_t* mboot_mods = NULL;

    /* CPU Initialization */
    debug_log("GDT / IDT initialization");
    gdt_initialize();
    idt_initialize();

    /* Terminal Initialization */
    debug_log("Terminal initialization");
    terminal_initialize();
    
    /* Multiboot check */
    if( magic != MULTIBOOT_BOOTLOADER_MAGIC )
    {
        KPANIC("INVALID MAGIC NUMBER", NULL);
    }
    mbi = (multiboot_info_t*)addr;
    initial_esp = esp;
    
    uintptr_t last_mod = (uintptr_t)&_kernel_end;
    if( CHECK_FLAG(mbi->flags, 5) ) /* mods */
    {
        debug_logf(debug_str, "There %s %d module%s starting at 0x%x", mbi->mods_count == 1 ? "is" : "are", mbi->mods_count, mbi->mods_count == 1 ? "" : "s", mbi->mods_addr);
        debug_logf(debug_str, "Current kernel heap start point would be 0x%x", &_kernel_end);
        if( mbi->mods_count > 0 )
        {
            uint32_t i;
            mboot_mods = (multiboot_module_t *)mbi->mods_addr;
            mboot_mods_count = mbi->mods_count;
            for( i = 0; i < mbi->mods_count; ++i )
            {
                multiboot_module_t* mod = &mboot_mods[i];
                uint32_t module_start = mod->mod_start;
                uint32_t module_end   = mod->mod_end;
                if( (uintptr_t)mod + sizeof(multiboot_module_t) > last_mod )
                {
                    /* Just in case some silly person put this *behind* the modules... */
                    last_mod = (uintptr_t)mod + sizeof(multiboot_module_t);
                    debug_logf(debug_str, "moving forward to 0x%x", last_mod);
                }
                debug_logf(debug_str, "Module %d is at 0x%x:0x%x", i, module_start, module_end);
                if( last_mod < module_end )
                {
                    last_mod = module_end;
                }
            }
            debug_logf(debug_str, "Moving kernel heap start to 0x%x", last_mod);
        }
    }
    if( (uintptr_t)mbi > last_mod )
    {
        last_mod = (uintptr_t)mbi + sizeof(multiboot_info_t);
    }
    while(last_mod & 0x7FF) last_mod++;
    kmalloc_startat(last_mod);
    
    /* Memory Initialization */
    if( !CHECK_FLAG(mbi->flags, 0) ) /* mem_upper/mem_lower  */
    {
        KPANIC("Missing MEM flag in multiboot header", NULL)
    }
    paging_initialize(mbi->mem_lower + mbi->mem_upper);

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
                    debug_logf(debug_str, "Marking 0x%x", (uint32_t)mmap->addr + i);
                    paging_mark_system((mmap->addr + i) & 0xFFFFF000);
                }
            }
            mmap = (multiboot_memory_map_t*)((uintptr_t)mmap + mmap->size + sizeof(uintptr_t));
        }
    }
    debug_log("Finalize paging / heap install");
    paging_finalize();
    
    char cmdline[1024];
    if( CHECK_FLAG(mbi->flags, 2) ) /* cmdline */
    {
        size_t len = strlen((char*)mbi->cmdline);
        memmove(cmdline, (char*)mbi->cmdline, len + 1);
    }

    heap_install();

    args_parse(cmdline);

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

    debug_log("Syscalls initialization");
    syscalls_initialize();

    debug_log("SHM initialization");
    shm_initialize();

    /* Test keyboard handler */
    debug_log("Install keyboard handler");
    keyboard_install();
    
    debug_log("Initialize fs types");
    tarfs_initialize();

    /* Load modules from bootloader */
    if( CHECK_FLAG(mbi->flags, 5) ) /* mods */
    {
        debug_logf(debug_str, "%d modules to load", mboot_mods_count);
        for( unsigned int i = 0; i < mbi->mods_count; ++i )
        {
            multiboot_module_t* mod = &mboot_mods[i];
            uint32_t module_start = mod->mod_start;
            uint32_t module_end = mod->mod_end;
            size_t   module_size = module_end - module_start;

            debug_logf(debug_str, "Loading ramdisk: 0x%x:0x%x", module_start, module_end);
            ramdisk_mount(module_start, module_size);
        }
    }
    
    map_vfs_directory("/dev");
    zero_initialize();
    
    debug_log("setup root mount");
    if( args_present("root") )
    {
        char* root_type = "ext2";
        if(args_present("root_type"))
        {
            root_type = args_value("root_type");
        }
        vfs_mount_type(root_type, args_value("root"), "/");
    }else
    {
        KPANIC("No root option given", NULL)
    }

    if( !fs_root )
    {
        map_vfs_directory("/");
    }

    char* boot_exec = "bin/init";
    char* boot_arg = NULL;
    char* argv[] =
    {
        boot_exec,
        boot_arg,
        NULL
    };

    int argc = 0;
    while( argv[argc] )
    {
        argc++;
    }

    system(argv[0], argc, argv, NULL);
    
    KPANIC("INIT FAILED", NULL);
}

void kpanic( char* error_message, const char* file, int line, struct regs* regs )
{
    int_disable();
    char debug_str[256];

    terminal_setcolor(4);

    printf("PANIC: %s ", error_message);
    debug_logf(debug_str, "PANIC: %s ", error_message);
    printf("File: %s ", file);
    debug_logf(debug_str, "File: %s ", file);
    printf("Line: %d ", line);
    debug_logf(debug_str, "Line: %d ", line);
    if(regs)
    {
        printf("\nREGISTERS:");
        debug_logf(debug_str, "\nREGISTERS:");
        printf("eax=0x%x ebx=0x%x\n", regs->eax, regs->ebx);
        debug_logf(debug_str, "eax=0x%x ebx=0x%x\n", regs->eax, regs->ebx);
        printf("ecx=0x%x edx=0x%x\n", regs->ecx, regs->edx);
        debug_logf(debug_str, "ecx=0x%x edx=0x%x\n", regs->ecx, regs->edx);
        printf("esp=0x%x ebp=0x%x\n", regs->esp, regs->ebp);
        debug_logf(debug_str, "esp=0x%x ebp=0x%x\n", regs->esp, regs->ebp);
        printf("ERRCD: 0x%x ", regs->err_code);
        debug_logf(debug_str, "ERRCD: 0x%x ", regs->err_code);
        printf("EFLAGS: 0x%x\n", regs->eflags);
        debug_logf(debug_str, "EFLAGS: 0x%x\n", regs->eflags);
        printf("User ESP: 0x%x ", regs->useresp);
        debug_logf(debug_str, "User ESP: 0x%x ", regs->useresp);
        printf("eip=0x%x\n", regs->eip);
        debug_logf(debug_str, "eip=0x%x\n", regs->eip);
    }
    
    int_disable();
    STOP
}
