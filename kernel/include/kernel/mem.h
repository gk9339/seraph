#ifndef _KERNEL_MEM_H
#define _KERNEL_MEM_H

#include <stddef.h> // size_t
#include <kernel/types.h> // struct regs
#include <stdint.h> // intN_t

#define USER_STACK_BOTTOM 0xAFF00000
#define USER_STACK_TOP 0xB0000000

extern uintptr_t heap_end;

typedef struct page
{
    unsigned int present:1;
    unsigned int rw:1;
    unsigned int user:1;
    unsigned int writethrough:1;
    unsigned int cachedisable:1;
    unsigned int accessed:1;
    unsigned int dirty:1;
    unsigned int pat:1;
    unsigned int global:1;
    unsigned int unused:3;
    unsigned int frame:20;
} __attribute__((packed)) page_t;

typedef struct page_table
{
    page_t pages[1024];
} page_table_t;

typedef struct page_directory
{
    uintptr_t physical_tables[1024]; // Physical addresses of the tables
    page_table_t* tables[1024]; // 1024 pointers to page tables
    uintptr_t physical_address; // The physical address of the tables
    uint32_t ref_count;
} page_directory_t;

void kmalloc_startat( uintptr_t address );
uintptr_t kmalloc_real( size_t size, int align, uintptr_t* phys );
uintptr_t kmalloc( size_t size );
uintptr_t kvmalloc( size_t size );
uintptr_t kmalloc_p( size_t size, uintptr_t* phys );
uintptr_t kvmalloc_p( size_t size, uintptr_t* phys );

extern page_directory_t* kernel_directory;
extern page_directory_t* current_directory;

extern uintptr_t placement_pointer;

void paging_initialize( uint32_t memsize );
void paging_prestart( void );
void paging_finalize( void );
void paging_mark_system( uint32_t addr );
void switch_page_directory( page_directory_t* new );
void invalidate_page_tables( void );
void invalidate_pages_at( uintptr_t addr );
void invalidate_tables_at( uintptr_t addr );
page_t* get_page( uintptr_t address, int make, page_directory_t* dir );
void page_fault( struct regs* r );
void alloc_frame( page_t*, int, int );
void dma_frame( page_t* page, int, int, uintptr_t );

void heap_install( void );

void* sbrk( uintptr_t increment );

void set_frame( uintptr_t frame_addr );
void clear_frame( uintptr_t frame_addr );
uint32_t test_frame( uintptr_t frame_addr );
uint32_t first_frame( void );

uintptr_t map_to_physical( uintptr_t virtual );

page_directory_t* clone_directory( page_directory_t* src );
void release_directory( page_directory_t* dir );
void release_directory_for_exec( page_directory_t* dir );
page_table_t* clone_table( page_table_t* src, uintptr_t* physAddr );
void move_stack( void* new_stack_start, size_t size );
void copy_page_physical( uint32_t, uint32_t );

uintptr_t memory_use( void );
uintptr_t memory_total( void );

#endif
