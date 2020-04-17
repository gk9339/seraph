#include <stdlib.h>
#include <string.h>
#include <stdio.h>

#include <kernel/mem.h>
#include <kernel/kernel.h>
#include <kernel/serial.h>
#include <kernel/spinlock.h>
#include <kernel/process.h>
#include <kernel/isr.h>
#include <kernel/shm.h>
#include <kernel/task.h>
#include <kernel/signal.h>

#define INDEX_FROM_BIT(b) (b / 0x20)
#define OFFSET_FROM_BIT(b) (b % 0x20)

uintptr_t placement_pointer = (uintptr_t)&_kernel_end;
uintptr_t heap_end = (uintptr_t)NULL;
uintptr_t kernel_heap_alloc_point = KERNEL_HEAP_INIT;

static spin_lock_t frame_alloc_lock = { 0 };
uint32_t first_n_frames(int n);

uint32_t* frames;
uint32_t nframes;

// Set start of kernel heap
void kmalloc_startat( uintptr_t address )
{
    placement_pointer = address;
}

uintptr_t kmalloc_real( size_t size, int align, uintptr_t* phys )
{
    if( heap_end )
    {
        void* address;
        if( align )
        {
            address = valloc(size);
        }else
        {
            address = malloc(size);
        }
        if( phys )
        {
            if( align && size >= 0x3000 )
            {
                /* Large alloc */
                for( uintptr_t i = (uintptr_t)address; i < (uintptr_t)address + size; i += 0x1000 )
                {
                    clear_frame(map_to_physical(i));
                }

                spin_lock(frame_alloc_lock);
                uint32_t index = first_n_frames((size + 0xFFF) / 0x1000);
                if( index == 0xFFFFFFFF )
                {
                    spin_unlock(frame_alloc_lock);
                    return 0;
                }

                for( unsigned int i = 0; i < (size + 0xFFF) / 0x1000; i++ )
                {
                    set_frame((index + i) * 0x1000);
                    page_t* page = get_page((uintptr_t)address + (i * 0x1000), 0, kernel_directory);
                    ASSUME(page != NULL);
                    page->frame = (index + i) & 0xfffff;
                    page->writethrough = 1;
                    page->cachedisable = 1;
                }
                spin_unlock(frame_alloc_lock);
            }
            *phys = map_to_physical((uintptr_t)address);
        }
        return (uintptr_t)address;
    }

    if( align && (placement_pointer & 0x00000FFF) )
    {
        placement_pointer &= 0xFFFFF000;
        placement_pointer += 0x1000;
    }
    if(phys)
    {
        *phys = placement_pointer;
    }
    uintptr_t address = placement_pointer;
    placement_pointer += size;
    
    return (uintptr_t)address;
}

uintptr_t kmalloc( size_t size )
{
    return kmalloc_real(size, 0, NULL);
}

uintptr_t kvmalloc( size_t size )
{
    return kmalloc_real(size, 1, NULL);
}

uintptr_t kmalloc_p( size_t size, uintptr_t* phys )
{
    return kmalloc_real(size, 0, phys);
}

uintptr_t kvmalloc_p( size_t size, uintptr_t* phys )
{
    return kmalloc_real(size, 1, phys);
}

void set_frame( uintptr_t frame_addr )
{
    if( frame_addr < nframes * 4 * 0x400 )
    {
        uint32_t frame = frame_addr / 0x1000;
        uint32_t index = INDEX_FROM_BIT(frame);
        uint32_t offset = OFFSET_FROM_BIT(frame);
        frames[index] |= ( (uint32_t)0x1 << offset );
    }
}

void clear_frame( uintptr_t frame_addr )
{
    uint32_t frame = frame_addr / 0x1000;
    uint32_t index = INDEX_FROM_BIT(frame);
    uint32_t offset = OFFSET_FROM_BIT(frame);
    frames[index] &= ~( (uint32_t)0x1 << offset );
}

uint32_t test_frame( uintptr_t frame_addr )
{
    uint32_t frame = frame_addr / 0x1000;
    uint32_t index = INDEX_FROM_BIT(frame);
    uint32_t offset = OFFSET_FROM_BIT(frame);
    return frames[index] & ( (uint32_t)0x1 << offset );
}

uint32_t first_n_frames( int n )
{
    for( uint32_t i = 0; i < nframes * 0x1000; i += 0x1000 )
    {
        int bad = 0;
        for( int j = 0; j < n; ++j )
        {
            if( test_frame(i + 0x1000 * j) )
                bad = j + 1;
        }
        if( !bad )
        {
            return i / 0x1000;
        }
    }
    return 0xFFFFFFFF;
}

uint32_t first_frame( void )
{
    uint32_t i, j;

    for( i = 0; i < INDEX_FROM_BIT(nframes); ++i )
    {
        if( frames[i] != 0xFFFFFFFF )
        {
            for( j = 0; j < 32; ++j )
            {
                uint32_t test_frame = (uint32_t)0x1 << j;
                if( !(frames[i] & test_frame) )
                {
                    return i * 0x20 + j;
                }
            }
        }
    }

    KPANIC("System out of memory", NULL);
    return -1;
}

void alloc_frame( page_t* page, int is_kernel, int is_writable )
{
    ASSUME(page != NULL);
    if( page->frame != 0 )
    {
        page->present = 1;
        page->rw = (is_writable == 1)?1:0;
        page->user = (is_kernel == 1)?0:1;
        return;
    }else
    {
        spin_lock(frame_alloc_lock);
        uint32_t index = first_frame();

        set_frame(index * 0x1000);
        page->frame = index & 0xfffff;
        spin_unlock(frame_alloc_lock);
        page->present = 1;
        page->rw = (is_writable == 1)?1:0;
        page->user = (is_kernel == 1)?0:1;
    }
}

void dma_frame( page_t* page, int is_kernel, int is_writable, uintptr_t address )
{
    ASSUME(page != NULL);
    page->present = 1;
    page->rw = (is_writable == 1)?1:0;
    page->user = (is_kernel == 1)?0:1;
    page->frame = (address / 0x1000) & 0xfffff;
    set_frame(address);
}

static void free_frame( page_t* page )
{
    uint32_t frame;
    if( !(frame = page->frame) )
    {
        return;
    }else
    {
        clear_frame(frame * 0x1000);
        page->frame = 0x0;
    }
}

uintptr_t memory_use( void )
{
    uintptr_t ret = 0;
    uint32_t i, j;
    for( i = 0; i< INDEX_FROM_BIT(nframes); ++i )
    {
        for( j = 0; j < 32; ++j )
        {
            uint32_t testFrame = (uint32_t)0x1 << j;
            if( frames[i] & testFrame )
                ret++;
        }
    }
    return ret * 4;
}

uintptr_t memory_total( void )
{
    return nframes * 4;
}

void paging_initialize( uint32_t memsize )
{
    nframes = memsize / 4;
    frames = (uint32_t*)kmalloc(INDEX_FROM_BIT(nframes * 8));
    memset(frames, 0, INDEX_FROM_BIT(nframes * 8));

    uintptr_t phys;
    kernel_directory = (page_directory_t*)kvmalloc_p(sizeof(page_directory_t), &phys);
    memset(kernel_directory, 0, sizeof(page_directory_t));

    /* Set PAT 111b to Write-combining */
    asm volatile(
            "mov $0x277, %%ecx\n" /* IA32_MSR_PAT */
            "rdmsr\n"
            "or $0x1000000, %%edx\n" /* Set bit 56 */
            "and $0xf9ffffff, %%edx\n" /* unset bits 57, 58 */
            "wrmsr\n"
            :::"ecx", "edx", "eax"
            );
}

void paging_mark_system( uint32_t addr )
{
    set_frame(addr);
}

void paging_finalize( void )
{
    get_page(0, 1, kernel_directory)->present = 0;
    set_frame(0);

    // Setup kernel frames read only
    for( uintptr_t i = 0x1000; i < placement_pointer + 0x3000; i += 0x1000 )
    {
        dma_frame(get_page(i, 1, kernel_directory), 1, 0, i);
    }
    
    // Mapping VGA text-mode 
    for( uintptr_t j = 0xb8000; j < 0xc0000; j += 0x1000 )
    {
        dma_frame(get_page(j, 0, kernel_directory), 0, 1, j);
    }

    // Install page fault handler
    isr_install_handler(14, page_fault);
    kernel_directory->physical_address = (uintptr_t)kernel_directory->physical_tables;

    uintptr_t tmp_heap_start = KERNEL_HEAP_INIT;

    if( tmp_heap_start <= placement_pointer + 0x3000 )
    {
        tmp_heap_start = placement_pointer + 0x100000;
        kernel_heap_alloc_point = tmp_heap_start;
    }

    // Kernel heap space
    for( uintptr_t i = placement_pointer + 0x3000; i < tmp_heap_start; i += 0x1000 )
    {
        alloc_frame(get_page(i, 1, kernel_directory), 1, 0);
    }

    // Preallocate page entries for the rest of the kernel heap
    for( uintptr_t i = tmp_heap_start; i < KERNEL_HEAP_END; i += 0x1000 )
    {
        get_page(i, 1, kernel_directory);
    }
    for( unsigned int i = 0xE000; i <= 0xFFF0; i += 0x40 )
    {
        get_page(i << 16UL, 1, kernel_directory);
    }

    current_directory = clone_directory(kernel_directory);
    switch_page_directory(kernel_directory);
}

uintptr_t map_to_physical( uintptr_t virtual )
{
    uintptr_t remaining = virtual % 0x1000;
    uintptr_t frame = virtual / 0x1000;
    uintptr_t table = frame / 1024;
    uintptr_t subframe = frame % 1024;

    if( current_directory->tables[table] )
    {
        page_t* p = &current_directory->tables[table]->pages[subframe];
        return p->frame * 0x1000 + remaining;
    }else
    {
        return 0;
    }
}

void switch_page_directory( page_directory_t* dir )
{
    current_directory = dir;
    asm volatile(
            "mov %0, %%cr3\n"
            "mov %%cr0, %%eax\n"
            "orl $0x80000000, %%eax\n"
            "mov %%eax, %%cr0\n"
            :: "r"(dir->physical_address)
            : "%eax"
            );
}

void invalidate_page_tables( void )
{
    asm volatile(
            "movl %%cr3, %%eax\n"
            "movl %%eax, %%cr3\n"
            ::: "eax"
            );
}

void invalidate_tables_at( uintptr_t addr )
{
    asm volatile(
            "movl %0, %%eax\n"
            "invlpg (%%eax)\n"
            :: "r"(addr):"%eax"
            );
}

page_t* get_page( uintptr_t address, int make, page_directory_t* dir )
{
    address /= 0x1000;
    uint32_t table_index = address / 1024;

    if( dir->tables[table_index] )
    {
        return &dir->tables[table_index]->pages[address % 1024];
    }else if( make )
    {
        uint32_t temp;
        dir->tables[table_index] = (page_table_t*)kvmalloc_p(sizeof(page_table_t), (uintptr_t*)(&temp));
        ASSUME(dir->tables[table_index] != NULL);
        memset(dir->tables[table_index], 0, sizeof(page_table_t));
        dir->physical_tables[table_index] = temp | 0x7;
        return &dir->tables[table_index]->pages[address % 1024];
    }else
    {
        return 0;
    }
}

void page_fault( struct regs* r )
{
    ASSUME(r != NULL);
    uint32_t faulting_address;
    char debug_str[128];
    asm volatile(
            "mov %%cr2, %0":"=r"(faulting_address)
            );

    if( r->eip == SIGNAL_RETURN )
    {
        return_from_signal_handler();
    }else if( r->eip == THREAD_RETURN )
    {
        kexit(0);
    }

    int present = !(r->err_code & 0x1)?1:0;
    int rw = r->err_code & 0x2 ?1:0;
    int user = r->err_code & 0x4 ?1:0;
    int reserved = r->err_code & 0x8 ?1:0;
    int id = r->err_code & 0x10 ?1:0;

    debug_logf(debug_str, "Segmentation Fault:(p:%d, rw:%d, user:%d, reserved:%d, id:%d) at 0x%x eip: 0x%x pid = %d, %d [%s]",
                present, rw, user, reserved, id, faulting_address, r->eip, current_process->id, current_process->group, current_process->name);

    send_signal(current_process->id, SIGSEGV, 1);
}

void heap_install( void )
{
    heap_end = (placement_pointer + 0x1000) & ~0xFFF;
}

void* sbrk( uintptr_t increment )
{
    if( (increment % 0x1000) != 0 ) KPANIC("Kernel tried to expand heap by non-page-multiple value", NULL);
    if( (heap_end % 0x1000) != 0 ) KPANIC("Kernel heap is not page aligned", NULL);
    if( (heap_end + increment > KERNEL_HEAP_END ) ) KPANIC("Kernel tried to allocate beyond the end of the heap", NULL);
    uintptr_t address = heap_end;

    if( heap_end + increment > kernel_heap_alloc_point )
    {
        for( uintptr_t i = heap_end; i < heap_end + increment; i += 0x1000 )
        {
            page_t* page = get_page(i, 0, kernel_directory);
            if( !page ) KPANIC("Kernel allocation fault", NULL);
            alloc_frame(page, 1, 0);
        }
        invalidate_page_tables();
    }

    heap_end += increment;
    memset((void*)address, 0x0, increment);
    return (void*)address;
}

page_directory_t* clone_directory( page_directory_t* src )
{
    uintptr_t dirphys;
    page_directory_t* dir = (page_directory_t*)kvmalloc_p(sizeof(page_directory_t), &dirphys);
    memset(dir, 0, sizeof(page_directory_t));
    dir->ref_count = 1;

    dir->physical_address = dirphys;
    uint32_t i;
    for( i = 0; i < 1024; ++i )
    {
        if( !src->tables[i] || (uintptr_t)src->tables[i] == (uintptr_t)0xFFFFFFFF )
        {
            continue;
        }

        if( kernel_directory->tables[i] == src->tables[i] )
        {
            dir->tables[i] = src->tables[i];
            dir->physical_tables[i] = src->physical_tables[i];
        }else
        {
            if( i * 0x1000 * 1024 < SHM_START )
            {
                uintptr_t phys;
                dir->tables[i] = clone_table(src->tables[i], &phys);
                dir->physical_tables[i] = phys | 0x07;
            }
        }
    }
    return dir;
}

void release_directory( page_directory_t* dir )
{
    dir->ref_count--;

    if(dir->ref_count < 1)
    {
        for( uint32_t i = 0; i < 1024; ++i )
        {
            if( !dir->tables[i] || (uintptr_t)dir->tables[i] == (uintptr_t)0xFFFFFFFF )
            {
                continue;
            }
            if( kernel_directory->tables[i] != dir->tables[i] )
            {
                if( i * 0x1000 * 1024 < SHM_START )
                {
                    for( uint32_t j = 0; j < 1024; ++j )
                    {
                        if( dir->tables[i]->pages[j].frame )
                        {
                            free_frame(&(dir->tables[i]->pages[j]));
                        }
                    }
                }
                free(dir->tables[i]);
            }
        }
        free(dir);
    }
}

void release_directory_for_exec( page_directory_t* dir )
{
    uint32_t i;

    for( i = 0; i < 1024; ++i )
    {
        if( !dir->tables[i] || (uintptr_t)dir->tables[i] == (uintptr_t)0xFFFFFFFF )
        {
            continue;
        }
        if( kernel_directory->tables[i] != dir->tables[i] )
        {
            if( i * 0x1000 * 1024 < USER_STACK_BOTTOM )
            {
                for( uint32_t j = 0; j < 1024; ++j )
                {
                    if( dir->tables[i]->pages[j].frame )
                    {
                        free_frame(&(dir->tables[i]->pages[j]));
                    }
                }
                dir->physical_tables[i] = 0;
                free(dir->tables[i]);
                dir->tables[i] = 0;
            }
        }
    }
}

page_table_t* clone_table( page_table_t* src, uintptr_t* physAddr )
{
    page_table_t* table = (page_table_t*)kvmalloc_p(sizeof(page_table_t), physAddr);
    memset(table, 0, sizeof(page_table_t));
    uint32_t i;
    
    for( i = 0; i < 1024; ++i )
    {
        if( !src->pages[i].frame )
        {
            continue;
        }

        alloc_frame(&table->pages[i], 0, 0);

        if( src->pages[i].present )         table->pages[i].present = 1;
        if( src->pages[i].rw )              table->pages[i].rw = 1;
        if( src->pages[i].user )            table->pages[i].user = 1;
        if( src->pages[i].writethrough )    table->pages[i].writethrough = 1;
        if( src->pages[i].cachedisable )    table->pages[i].cachedisable = 1;

        copy_page_physical(src->pages[i].frame * 0x1000, table->pages[i].frame * 0x1000);
    }

    return table;
}
