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
#include <kernel/keyboard.h>
#include <kernel/kconfig.h>
#include <kernel/acpi.h>
#include <kernel/random.h>
#include <kernel/tmpfs.h>
#include <sys/types.h>
#include <kernel/lfb.h>
#include <kernel/ata.h>
#include <kernel/mbr.h>

uintptr_t initial_esp = 0;
int debug = 0;
multiboot_info_t* mbi;
char kcmdline[1024];

void kernel_main( unsigned long magic, unsigned long addr, uintptr_t esp ) 
{
    char debug_str[256];
    uint32_t mboot_mods_count = 0;
    multiboot_module_t* mboot_mods = NULL;

    // Multiboot check
    if( magic != MULTIBOOT_BOOTLOADER_MAGIC )
    {
        KPANIC("INVALID MAGIC NUMBER", NULL);
    }
    mbi = (multiboot_info_t*)addr;
    initial_esp = esp;
    
    // Parse cmdline
    if( CHECK_FLAG(mbi->flags, 2) ) // cmdline
    {
        size_t len = strlen((char*)mbi->cmdline);
        memmove(kcmdline, (char*)mbi->cmdline, len + 1);
    }else
    {
        kcmdline[0] = 0;
    }
    
    // Parse args
    args_parse(kcmdline);
    if( args_present("serialdebug") )
    {
        debug = 1;
    }
    
    if( CHECK_FLAG(debug, 0) ) debug_log("\nStart kernel_main");

    // CPU Initialization
    if( CHECK_FLAG(debug, 0) ) debug_log("GDT / IDT initialization");
    gdt_initialize();
    idt_initialize();

    if( CHECK_FLAG(debug, 0) ) debug_log("ACPI Initializing");
    initialize_acpi();

    // Terminal Initialization
    if( CHECK_FLAG(debug, 0) ) debug_log("Terminal initialization");
    lfb_initialize();
    // Check if output to lfb is requested
    if( args_present("splash") )
    {
        debug = (uint8_t)(debug + 2);
    }
    if( CHECK_FLAG(debug, 1) ) printf("Kernel Initializing\n");

    // Multiboot boot device
    if (CHECK_FLAG (mbi->flags, 1))
    {
        if( CHECK_FLAG(debug, 0) ) debug_logf(debug_str, "boot_device = %#p\n", (unsigned) mbi->boot_device);
        if( CHECK_FLAG(debug, 1) ) printf("boot_device = %#p\n", (unsigned) mbi->boot_device);
    }

    // Multiboot modules
    uintptr_t last_mod = (uintptr_t)&_kernel_end;
    if( CHECK_FLAG(mbi->flags, 5) ) // mods
    {
        if( CHECK_FLAG(debug, 0) ) debug_logf(debug_str, "Parsing multiboot modules");
        if( CHECK_FLAG(debug, 1) ) printf("Parsing multiboot modules\n");
        if( CHECK_FLAG(debug, 0) ) debug_logf(debug_str, "There %s %d module%s starting at %#p", mbi->mods_count == 1 ? "is" : "are", mbi->mods_count, mbi->mods_count == 1 ? "" : "s", mbi->mods_addr);
        if( CHECK_FLAG(debug, 1) ) printf("There %s %d module%s starting at %#p\n", mbi->mods_count == 1 ? "is" : "are", mbi->mods_count, mbi->mods_count == 1 ? "" : "s", mbi->mods_addr);
        if( CHECK_FLAG(debug, 0) ) debug_logf(debug_str, "Current kernel heap start point would be %#p", &_kernel_end);
        if( CHECK_FLAG(debug, 1) ) printf("Current kernel heap start point would be %#p\n", &_kernel_end);

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
                    last_mod = (uintptr_t)mod + sizeof(multiboot_module_t);
                    if( CHECK_FLAG(debug, 0) ) debug_logf(debug_str, "moving forward to %#p", last_mod);
                    if( CHECK_FLAG(debug, 1) ) printf(debug_str, "moving forward to %#p\n", last_mod);
                }
                if( CHECK_FLAG(debug, 0) ) debug_logf(debug_str, "Module %d is at %#p:%#p", i, module_start, module_end);
                if( CHECK_FLAG(debug, 1) ) printf("Module %d is at %#p:%#p\n", i, module_start, module_end);
                if( last_mod < module_end )
                {
                    last_mod = module_end;
                }
            }
            if( CHECK_FLAG(debug, 0) ) debug_logf(debug_str, "Moving kernel heap start to %#p", last_mod);
            if( CHECK_FLAG(debug, 1) ) printf("Moving kernel heap start to %#p\n", last_mod);
        }
    }

    if( (uintptr_t)mbi > last_mod )
    {
        last_mod = (uintptr_t)mbi + sizeof(multiboot_info_t);
    }
    while(last_mod & 0x7FF)
    {
        last_mod++;
    }
    kmalloc_startat(last_mod);

    // Memory initialization
    if( !CHECK_FLAG(mbi->flags, 0) ) // mem_upper/mem_lower
    {
        KPANIC("Missing MEM flag in multiboot header", NULL)
    }
    paging_initialize(mbi->mem_lower + mbi->mem_upper);

    if( CHECK_FLAG(mbi->flags, 6) ) // mmap
    {
        if( CHECK_FLAG(debug, 0) ) debug_log("\nParsing memory map");
        if( CHECK_FLAG(debug, 1) ) printf("Parsing memory map\n");
        multiboot_memory_map_t* mmap = (void*)mbi->mmap_addr;
        
        int memory_mark_counter = 0;
        if( CHECK_FLAG(debug, 0) ) debug_log("(0)");
        if( CHECK_FLAG(debug, 1) ) printf("(0)");
        
        while( (uintptr_t)mmap < mbi->mmap_addr + mbi->mmap_length )
        {
            if(mmap->type == 2)
            {
                for( unsigned long int i = 0; i < mmap->len; i+= 0x1000 )
                {
                    if( mmap->addr + i > 0xFFFFFFFF ) break;
                    memory_mark_counter++;
                    paging_mark_system((mmap->addr + i) & 0xFFFFF000);
                }
            }
            mmap = (multiboot_memory_map_t*)((uintptr_t)mmap + mmap->size + sizeof(uintptr_t));
        }
        
        if( CHECK_FLAG(debug, 0) ) debug_logf(debug_str, "\033[F(%d)", memory_mark_counter);
        if( CHECK_FLAG(debug, 1) ) printf("\r(%d) marked pages", memory_mark_counter);
    }
    
    if( CHECK_FLAG(debug, 0) ) debug_log("Finalize paging");
    if( CHECK_FLAG(debug, 1) ) printf("\nFinalize paging\n");
    paging_finalize();

    heap_install();
    lfb_initialize();
    if( CHECK_FLAG(debug, 0) ) debug_log("Heap install\n");
    if( CHECK_FLAG(debug, 1) ) printf("Heap install\n");

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
    
    if( CHECK_FLAG(debug, 0) ) debug_log("Initialize fs types");
    if( CHECK_FLAG(debug, 1) ) printf("Initialize fs types\n");
    ustar_initialize();
    procfs_initialize();
    tmpfs_initialize();

    if( CHECK_FLAG(debug, 0) ) debug_log("Initialize device drivers\n");
    if( CHECK_FLAG(debug, 1) ) printf("Initialize device drivers\n");
    ata_initialize();
    if( CHECK_FLAG(debug, 0) ) debug_log("Initialize mbr\n");
    if( CHECK_FLAG(debug, 1) ) printf("Initialize mbr\n");
    mbr_initialize();
    
    // Load modules from bootloader
    if( CHECK_FLAG(mbi->flags, 5) ) // mods
    {
        if( CHECK_FLAG(debug, 0) ) debug_logf(debug_str, "Loading multiboot modules - %d modules to load", mboot_mods_count);
        if( CHECK_FLAG(debug, 1) ) printf("Loading multiboot modules - %d modules to load\n", mboot_mods_count);
        for( unsigned int i = 0; i < mbi->mods_count; ++i )
        {
            multiboot_module_t* mod = &mboot_mods[i];
            uint32_t module_start = mod->mod_start;
            uint32_t module_end = mod->mod_end;
            size_t   module_size = module_end - module_start;

            if( CHECK_FLAG(debug, 0) ) debug_logf(debug_str, "Loading ramdisk: %#p:%#p", module_start, module_end);
            if( CHECK_FLAG(debug, 1) ) printf("Loading ramdisk: %#p:%#p\n", module_start, module_end);
            ramdisk_mount(module_start, module_size);
        }
    }
    
    // Root mount
    if( CHECK_FLAG(debug, 0) ) debug_log("\nSetup root mount");
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
    
    // Virtual dev filesystem
    if( CHECK_FLAG(debug, 0) ) debug_log("Setup /dev");
    if( CHECK_FLAG(debug, 1) ) printf("Setup /dev\n");
    map_vfs_directory("/dev");
    lfb_initialize_device();
    zero_initialize();
    null_initialize();
    random_initialize();

    if( CHECK_FLAG(debug, 0) ) debug_log("Setup /proc");
    if( CHECK_FLAG(debug, 1) ) printf("Setup /proc\n");
    vfs_mount_type("procfs", "proc", "/proc");

    if( CHECK_FLAG(debug, 0) ) debug_log("Setup /tmp");
    if( CHECK_FLAG(debug, 1) ) printf("Setup /tmp\n");
    vfs_mount_type("tmpfs", "tmp", "/tmp");

    // Set up environment for /bin/init
    char* boot_exec = "/bin/init";
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

    // Start /bin/init
    if( CHECK_FLAG(debug, 0) ) debug_log("Starting /bin/init\n");
    if( CHECK_FLAG(debug, 1) ) printf("Starting /bin/init\n");
    sys_exec(argv[0], argc, argv, NULL);
    
    // Something went very wrong
    KPANIC("INIT FAILED", NULL);
}

void kpanic( char* error_message, const char* file, int line, struct regs* regs )
{
    int_disable();
    char debug_str[256];

    terminal_kpanic_color();

    printf("\fUNHANDLED EXCEPTION: %s ", error_message);
    debug_logf(debug_str, "UNHANDLED EXCEPTION: %s ", error_message);
    printf("pid: %d ", getpid());
    debug_logf(debug_str, "pid: %d", getpid());
    printf("File: %s ", file);
    debug_logf(debug_str, "File: %s ", file);
    printf("Line: %d ", line);
    debug_logf(debug_str, "Line: %d ", line);
    if(regs)
    {
        printf("\nREGISTERS:");
        debug_logf(debug_str, "\nREGISTERS:");
        printf("eax=%#p ebx=%#p\n", regs->eax, regs->ebx);
        debug_logf(debug_str, "eax=%#p ebx=%#p", regs->eax, regs->ebx);
        printf("ecx=%#p edx=%#p\n", regs->ecx, regs->edx);
        debug_logf(debug_str, "ecx=%#p edx=%#p", regs->ecx, regs->edx);
        printf("esp=%#p ebp=%#p\n", regs->esp, regs->ebp);
        debug_logf(debug_str, "esp=%#p ebp=%#p", regs->esp, regs->ebp);
        printf("ERRCD: %#p ", regs->err_code);
        debug_logf(debug_str, "ERRCD: %#p ", regs->err_code);
        printf("EFLAGS: %#p\n", regs->eflags);
        debug_logf(debug_str, "EFLAGS: %#p", regs->eflags);
        printf("User ESP: %#p ", regs->useresp);
        debug_logf(debug_str, "User ESP: %#p ", regs->useresp);
        printf("eip=%#p\n", regs->eip);
        debug_logf(debug_str, "eip=%#p", regs->eip);
        printf("int_no=%#p\n", regs->int_no);
        debug_logf(debug_str, "int_no=%#p", regs->int_no);
    }
    send_signal(current_process->id, SIGKILL, 1);
}
