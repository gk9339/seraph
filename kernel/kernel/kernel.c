#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <kernel/kernel.h>
#include <kernel/multiboot.h>
#include <kernel/gdt.h>
#include <kernel/idt.h>
#include <kernel/isr.h>
#include <kernel/irq.h>
#include <kernel/mem.h>
#include <kernel/args.h>
#include <kernel/fs.h>
#include <kernel/ramdisk.h>
#include <kernel/ustar.h>
#include <kernel/ext2.h>
#include <kernel/task.h>
#include <kernel/elf.h>
#include <kernel/process.h>
#include <kernel/signal.h>
#include <kernel/timer.h>
#include <kernel/cmos.h>
#include <kernel/fpu.h>
#include <kernel/syscall.h>
#include <kernel/shm.h>
#include <kernel/serial.h>
#include <kernel/vga.h>
#include <kernel/keyboard.h>
#include <kernel/kconfig.h>
#include <sys/types.h>

uintptr_t initial_esp = 0;
int debug = 0;

void kernel_main( unsigned long magic, unsigned long addr, uintptr_t esp ) 
{
    char debug_str[256];
    uint32_t mboot_mods_count = 0;
    multiboot_info_t* mbi;
    multiboot_module_t* mboot_mods = NULL;
#if EARLY_KERNEL_DEBUG
    debug = 1; // Required for debug_log to output, will be unset before parsing args
    debug_log("Start kernel_main");
#endif

    // CPU Initialization
#if EARLY_KERNEL_DEBUG
    debug_log("GDT / IDT initialization");
#endif
    gdt_initialize();
    idt_initialize();

    // Multiboot check
    if( magic != MULTIBOOT_BOOTLOADER_MAGIC )
    {
        KPANIC("INVALID MAGIC NUMBER", NULL);
    }
    mbi = (multiboot_info_t*)addr;
    initial_esp = esp;
 
    // Terminal Initialization
    debug_log("Terminal initialization");
    terminal_initialize();
    printf("Kernel Initializing\n");

#if EARLY_KERNEL_DEBUG
    // Multiboot boot device
    if (CHECK_FLAG (mbi->flags, 1))
    {
        debug_logf(debug_str, "boot_device = 0x%x\n", (unsigned) mbi->boot_device);
    }
#endif

    // Multiboot modules
    uintptr_t last_mod = (uintptr_t)&_kernel_end;
    if( CHECK_FLAG(mbi->flags, 5) ) /* mods */
    {
#if EARLY_KERNEL_DEBUG
        debug_logf(debug_str, "There %s %d module%s starting at 0x%x", mbi->mods_count == 1 ? "is" : "are", mbi->mods_count, mbi->mods_count == 1 ? "" : "s", mbi->mods_addr);
        printf("There %s %d module%s starting at 0x%x\n", mbi->mods_count == 1 ? "is" : "are", mbi->mods_count, mbi->mods_count == 1 ? "" : "s", mbi->mods_addr);
        debug_logf(debug_str, "Current kernel heap start point would be 0x%x", &_kernel_end);
        printf("Current kernel heap start point would be 0x%x\n", &_kernel_end);
#endif
        if( mbi->mods_count > 0 )
        {
            uint32_t i;
            mboot_mods = (multiboot_module_t *)mbi->mods_addr;
            mboot_mods_count = mbi->mods_count;
            for( i = 0; i < mbi->mods_count; ++i )
            {
                multiboot_module_t* mod = &mboot_mods[i];
#if EARLY_KERNEL_DEBUG
                uint32_t module_start = mod->mod_start;
#endif
                uint32_t module_end   = mod->mod_end;
                if( (uintptr_t)mod + sizeof(multiboot_module_t) > last_mod )
                {
                    last_mod = (uintptr_t)mod + sizeof(multiboot_module_t);
#if EARLY_KERNEL_DEBUG
                    debug_logf(debug_str, "moving forward to 0x%x", last_mod);
                    printf(debug_str, "moving forward to 0x%x\n", last_mod);
#endif
                }
#if EARLY_KERNEL_DEBUG
                debug_logf(debug_str, "Module %d is at 0x%x:0x%x", i, module_start, module_end);
                printf("Module %d is at 0x%x:0x%x\n", i, module_start, module_end);
#endif
                if( last_mod < module_end )
                {
                    last_mod = module_end;
                }
            }
#if EARLY_KERNEL_DEBUG
            debug_logf(debug_str, "Moving kernel heap start to 0x%x", last_mod);
            printf("Moving kernel heap start to 0x%x\n", last_mod);
#endif
        }
    }
    if( (uintptr_t)mbi > last_mod )
    {
        last_mod = (uintptr_t)mbi + sizeof(multiboot_info_t);
    }
    while(last_mod & 0x7FF) last_mod++;
    kmalloc_startat(last_mod);
    
    // Memory initialization
    if( !CHECK_FLAG(mbi->flags, 0) ) // mem_upper/mem_lower
    {
        KPANIC("Missing MEM flag in multiboot header", NULL)
    }
    paging_initialize(mbi->mem_lower + mbi->mem_upper);

    if( CHECK_FLAG(mbi->flags, 6) ) // mmap
    {
#if EARLY_KERNEL_DEBUG
        debug_log("\nParsing memory map.");
        printf("Parsing memory map.\n");
#endif
        multiboot_memory_map_t* mmap = (void*)mbi->mmap_addr;
#if EARLY_KERNEL_DEBUG
        int memory_mark_counter = 0;
        debug_log("(0)");
        printf("(0)");
#endif
        while( (uintptr_t)mmap < mbi->mmap_addr + mbi->mmap_length )
        {
            if(mmap->type == 2)
            {
                for( unsigned long int i = 0; i < mmap->len; i+= 0x1000 )
                {
                    if( mmap->addr + i > 0xFFFFFFFF ) break;
#if EARLY_KERNEL_DEBUG
                    debug_logf(debug_str, "\033[F(%d)", ++memory_mark_counter);
                    printf("\r(%d)", ++memory_mark_counter);
#endif
                    paging_mark_system((mmap->addr + i) & 0xFFFFF000);
                }
            }
            mmap = (multiboot_memory_map_t*)((uintptr_t)mmap + mmap->size + sizeof(uintptr_t));
        }
    }
#if EARLY_KERNEL_DEBUG
    debug_log("Finalize paging / heap install\n");
    printf("\nFinalize paging / heap install\n");
    debug = 0; // Enabled manually by EARLY_KERNEL_DEBUG earlier
#endif
    paging_finalize();

    // Parse cmdline
    char cmdline[1024];
    if( CHECK_FLAG(mbi->flags, 2) ) // cmdline
    {
        size_t len = strlen((char*)mbi->cmdline);
        memmove(cmdline, (char*)mbi->cmdline, len + 1);
    }

    heap_install();
    
    // Parse args
    args_parse(cmdline);
    if( args_present("serialdebug") )
    {
        debug = 1;
    }
    if( args_present("verbose") )
    {
        debug = (uint8_t)(debug + 2);
    }

    // Interrupts Initialization
    if( CHECK_FLAG(debug, 0) ) debug_log("Interrupts initialization");
    if( CHECK_FLAG(debug, 1) ) printf("Interrupts initialization\n");
    isr_initialize();
    irq_initialize();
    
    if( CHECK_FLAG(debug, 0) ) debug_log("VFS initialization");
    if( CHECK_FLAG(debug, 1) ) printf("VFS initialization\n");
    vfs_initialize();

    if( CHECK_FLAG(debug, 0) ) debug_log("Tasking initialization");
    if( CHECK_FLAG(debug, 1) ) printf("Tasking initialization\n");
    tasking_initialize();

    if( CHECK_FLAG(debug, 0) ) debug_log("Timer initialization");
    if( CHECK_FLAG(debug, 1) ) printf("Timer initialization\n");
    timer_initialize();

    if( CHECK_FLAG(debug, 0) ) debug_log("FPU initialization");
    if( CHECK_FLAG(debug, 1) ) printf("FPU initialization\n");
    fpu_initialize();

    if( CHECK_FLAG(debug, 0) ) debug_log("Syscalls initialization");
    if( CHECK_FLAG(debug, 1) ) printf("Syscalls initialization\n");
    syscalls_initialize();

    if( CHECK_FLAG(debug, 0) ) debug_log("SHM initialization");
    if( CHECK_FLAG(debug, 1) ) printf("SHM initialization\n");
    shm_initialize();

    if( CHECK_FLAG(debug, 0) ) debug_log("Install keyboard handler");
    if( CHECK_FLAG(debug, 1) ) printf("Install keyboard handler\n");
    keyboard_install();
    
    if( CHECK_FLAG(debug, 0) ) debug_log("Initialize fs types\n");
    if( CHECK_FLAG(debug, 1) ) printf("Initialize fs types\n");
    ustar_initialize();

    // Load modules from bootloader
    if( CHECK_FLAG(mbi->flags, 5) ) // mods
    {
        if( CHECK_FLAG(debug, 0) ) debug_logf(debug_str, "%d modules to load", mboot_mods_count);
        if( CHECK_FLAG(debug, 1) ) printf("%d modules to load\n", mboot_mods_count);
        for( unsigned int i = 0; i < mbi->mods_count; ++i )
        {
            multiboot_module_t* mod = &mboot_mods[i];
            uint32_t module_start = mod->mod_start;
            uint32_t module_end = mod->mod_end;
            size_t   module_size = module_end - module_start;

            if( CHECK_FLAG(debug, 0) ) debug_logf(debug_str, "Loading ramdisk: 0x%x:0x%x", module_start, module_end);
            if( CHECK_FLAG(debug, 1) ) printf("Loading ramdisk: 0x%x:0x%x\n", module_start, module_end);
            ramdisk_mount(module_start, module_size);
        }
    }
    
    // Virtual dev filesystem
    if( CHECK_FLAG(debug, 0) ) debug_log("\nSetup /dev");
    if( CHECK_FLAG(debug, 1) ) printf("\nSetup /dev\n");
    map_vfs_directory("/dev");
    zero_initialize();
    null_initialize();
    
    // ramfs initialization
    if( CHECK_FLAG(debug, 0) ) debug_log("Setup root mount");
    if( CHECK_FLAG(debug, 1) ) printf("Setup root mount\n");
    if( args_present("root") )
    {
        char* root_type = "ext2";
        if(args_present("root_type"))
        {
            root_type = args_value("root_type");
        }
        if( CHECK_FLAG(debug, 0) ) debug_logf(debug_str, "Root type = %s", root_type);
        if( CHECK_FLAG(debug, 1) ) printf("Root type = %s\n", root_type);
        vfs_mount_type(root_type, args_value("root"), "/");
    }else
    {
        KPANIC("No root option given", NULL)
    }

    if( !fs_root )
    {
        KPANIC("Root mount failed", NULL)
    }

    // Set up environment for /sbin/init
    char* boot_exec = "/sbin/init";
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

    // Start /sbin/init
    if( CHECK_FLAG(debug, 0) ) debug_log("Starting /sbin/init\n");
    if( CHECK_FLAG(debug, 1) ) printf("Starting /sbin/init\n");
    system(argv[0], argc, argv, NULL);
    
    // Something went very wrong
    KPANIC("INIT FAILED", NULL);
}

void kpanic( char* error_message, const char* file, int line, struct regs* regs )
{
    int_disable();
    char debug_str[256];

    terminal_setcolor(4);

    printf("\nUNHANDLED EXCEPTION: %s ", error_message);
    debug_logf(debug_str, "UNHANDLED EXCEPTION: %s ", error_message);
    printf("pid: %d", getpid());
    debug_logf(debug_str, "pid: %d", getpid());
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
    send_signal(current_process->id, SIGKILL, 1);
}
