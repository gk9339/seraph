#include <stdio.h>

#include <kernel/kernel.h>
#include <kernel/multiboot.h>
#include <kernel/tty.h>
#include <kernel/gdt.h>
#include <kernel/idt.h>
#include <kernel/isr.h>
#include <kernel/irq.h>

#define CHECK_FLAG(flags,bit) ((flags)&(1<<(bit)))

void kernel_main( unsigned long magic, unsigned long addr ) 
{
    multiboot_info_t* mbi;

    /* CPU Initialization */
    gdt_initialize();
    idt_initialize();
    isr_initialize();
    irq_initialize();

    /* Initialize terminal interface */
    terminal_initialize();

    if( magic != MULTIBOOT_BOOTLOADER_MAGIC )
    {
        KPANIC("INVALID MAGIC NUMBER", NULL);
    }
    mbi = (multiboot_info_t*)addr;

    printf("All char test:\n");

	char str[1];
    for( int i = 32; i <= 255; i++ )
    {
        str[0] = i;
            printf("%s",str);
    }
    
    printf("\n\nflags = 0x%x\n", (unsigned)mbi->flags);

    if( CHECK_FLAG(mbi->flags, 0) )
    {
        printf("mem_lower = 0x%x, mem_upper = 0x%x\n", 
                (unsigned)mbi->mem_lower, (unsigned)mbi->mem_upper);
    }

    if( CHECK_FLAG(mbi->flags, 1) )
    {
        printf("boot_device = 0x%x\n", (unsigned)mbi->boot_device);
    }

    if( CHECK_FLAG(mbi->flags, 2) )
    {
        printf("cmdline = %s\n", (char*)mbi->cmdline);
    }

    if( CHECK_FLAG(mbi->flags, 3) )
    {
        multiboot_module_t* mod;
        int i;

        printf("mods_count = %x, mods_addr = 0x%x\n",
                (int)mbi->mods_count, (int)mbi->mods_addr);
        for( i = 0, mod = (multiboot_module_t*)mbi->mods_addr;
                i < mbi->mods_count; i++ )
        {
            printf("mod_start = 0x%x, mod_end = 0x%x, cmdline = %s\n",
                    (unsigned)mod->mod_start,
                    (unsigned)mod->mod_end,
                    (char*)mod->cmdline);
        }
    }

    if( CHECK_FLAG (mbi->flags, 4) && CHECK_FLAG (mbi->flags, 5) )
    {
        printf ("Both bits 4 and 5 are set.\n");
        return;
    }

    /* Is the symbol table of a.out valid? */
    if( CHECK_FLAG (mbi->flags, 4) )
    {
        multiboot_aout_symbol_table_t *multiboot_aout_sym = &(mbi->u.aout_sym);

        printf ("multiboot_aout_symbol_table: tabsize = 0x%0x, "
                "strsize = 0x%x, addr = 0x%x\n",
                (unsigned) multiboot_aout_sym->tabsize,
                (unsigned) multiboot_aout_sym->strsize,
                (unsigned) multiboot_aout_sym->addr);
    }

    /* Is the section header table of ELF valid? */
    if( CHECK_FLAG (mbi->flags, 5) )
    {
        multiboot_elf_section_header_table_t *multiboot_elf_sec = &(mbi->u.elf_sec);

        printf ("multiboot_elf_sec: num = %x, size = 0x%x,"
                " addr = 0x%x, shndx = 0x%x\n",
                (unsigned) multiboot_elf_sec->num, (unsigned) multiboot_elf_sec->size,
                (unsigned) multiboot_elf_sec->addr, (unsigned) multiboot_elf_sec->shndx);
    }

    if( CHECK_FLAG(mbi->flags, 6) )
    {
        multiboot_memory_map_t* mmap;

        printf("mmap_addr = 0x%x, mmap_length = 0x%x\n",
                (unsigned)mbi->mmap_addr, (unsigned)mbi->mmap_length);
        for(mmap = (multiboot_memory_map_t*)mbi->mmap_addr;
                (unsigned long)mmap < mbi->mmap_addr + mbi->mmap_length;
                mmap = (multiboot_memory_map_t*)((unsigned long)mmap + 
                    mmap->size + sizeof(mmap->size)))
        {
            printf("size = 0x%x, base_addr = 0x%x %x, length = 0x%x %x, type = 0x%x\n",
                    (unsigned)mmap->size,
                    (unsigned)(mmap->addr >>32),
                    (unsigned)(mmap->addr & 0xffffffff),
                    (unsigned)(mmap->len >> 32),
                    (unsigned)(mmap->len & 0xffffffff),
                    (unsigned)mmap->type);
        }
    }
}

void kpanic( char* error_message, const char* file, int line, struct regs* regs )
{
    printf("PANIC: %s ", error_message);
    printf("File: %s ", file);
    printf("Line: %x ", line);
    if(regs)
    {
        printf("\nREGISTERS:");
        printf("eax=0x%d ebx=0x%d\n", regs->eax, regs->ebx);
        printf("ecx=0x%d edx=0x%d\n", regs->ecx, regs->edx);
        printf("esp=0x%d ebp=0x%d\n", regs->esp, regs->ebp);
        printf("ERRCD: 0x%d", regs->err_code);
        printf("EFLAGSL 0x%d", regs->eflags);
        printf("User ESP: 0x%d", regs->useresp);
        printf("eip=0x%d", regs->eip);
    }
}
