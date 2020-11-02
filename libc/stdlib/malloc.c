#include <stdlib.h>
#include <string.h>
#include <limits.h>
#ifdef __is_libk
#include <kernel/kernel.h> // KPANIC
#include <kernel/mem.h>
#include <kernel/spinlock.h>
#include <kernel/serial.h>
#else
#include <spinlock.h>
#include <unistd.h>
#include <sys/syscall.h>
#include <debug.h>
#endif 

#define NUM_BINS 11U
#define SMALLEST_BIN_LOG 2U
#define BIG_BIN (NUM_BINS - 1)
#define SMALLEST_BIN (1UL << SMALLEST_BIN_LOG)

#define PAGE_SIZE 0x1000
#define PAGE_MASK (PAGE_SIZE - 1)
#define SKIP_P LONG_MAX
#define SKIP_MAX_LEVEL 6

#define BIN_MAGIC 0x9e928088

// Internal functions 
static void* __attribute__((malloc)) malloc_i( uintptr_t size );
static void* __attribute__((malloc)) realloc_i( void* ptr, uintptr_t size );
static void* __attribute__((malloc)) calloc_i( uintptr_t nmemb, uintptr_t size );
static void* __attribute__((malloc)) valloc_i( uintptr_t size );
static void free_i( void* ptr );

#ifndef __is_libk
#define KPANIC(mesg, regs) abort()
#endif

static spin_lock_t mem_lock = { 0 };

void* __attribute__((malloc)) malloc( uintptr_t size )
{
    spin_lock(mem_lock);
    void* ret = malloc_i(size);
    spin_unlock(mem_lock);
    
    return ret;
}

void* __attribute__((malloc)) realloc( void* ptr, uintptr_t size )
{
    spin_lock(mem_lock);
    void* ret = realloc_i(ptr, size);
    spin_unlock(mem_lock);

    return ret;
}

void* __attribute__((malloc)) calloc( uintptr_t nmemb, uintptr_t size )
{
    spin_lock(mem_lock);
    void* ret = calloc_i(nmemb, size);
    spin_unlock(mem_lock);

    return ret;
}

void* __attribute__((malloc)) valloc( uintptr_t size )
{
    spin_lock(mem_lock);
    void* ret = valloc_i(size);
    spin_unlock(mem_lock);

    return ret;
}

void free( void* ptr )
{
    spin_lock(mem_lock);
    free_i(ptr);
    spin_unlock(mem_lock);
}

// Bin management 
// Adjust bin size in bin_size call to proper bounds 
inline static uintptr_t __attribute__((always_inline, pure)) malloc_i_adjust_bin( uintptr_t bin )
{
    if( bin <= (uintptr_t)SMALLEST_BIN_LOG )
    {
        return 0;
    }
    bin -= SMALLEST_BIN_LOG + 1;
    if( bin > (uintptr_t)BIG_BIN )
    {
        return BIG_BIN;
    }

    return bin;
}

// Given a size value, find the correct bin to place the allocation in 
inline static uintptr_t __attribute__((always_inline, pure)) malloc_i_bin_size( uintptr_t size )
{
    uintptr_t bin = sizeof(size) * CHAR_BIT - __builtin_clzl(size);
    bin += !!(size & (size -1 ));
    
    return malloc_i_adjust_bin(bin);
}

// Bin header 
typedef struct _malloc_i_bin_header
{
    struct _malloc_i_bin_header* next;
    void* head;
    uintptr_t size;
    uint32_t bin_magic;
} malloc_i_bin_header_t;

// Big bin header, different pointers 
typedef struct _malloc_i_big_bin_header
{
    struct _malloc_i_big_bin_header* next;
    void* head;
    uintptr_t size;
    uint32_t bin_magic;
    struct _malloc_i_big_bin_header* prev;
    struct _malloc_i_big_bin_header* forward[SKIP_MAX_LEVEL+1];
} malloc_i_big_bin_header_t;

// List of pages in a bin 
typedef struct _malloc_i_bin_header_head
{
    malloc_i_bin_header_t* first;
} malloc_i_bin_header_head_t;

// Array of available bins 
static malloc_i_bin_header_head_t malloc_i_bin_head[NUM_BINS - 1];
static struct _malloc_i_big_bins
{
    malloc_i_big_bin_header_t head;
    int level;
} malloc_i_big_bins;
static malloc_i_big_bin_header_t* malloc_i_newest_big = NULL;

// Doubly-linked list 
// Remove an entry from a page list 
inline static void __attribute__((always_inline)) malloc_i_list_decouple( malloc_i_bin_header_head_t* head, malloc_i_bin_header_t* node )
{
    malloc_i_bin_header_t* next = node->next;
    head->first = next;
    node->next = NULL;
}

// Insert an entry into a page list 
inline static void __attribute__((always_inline)) malloc_i_list_insert( malloc_i_bin_header_head_t* head, malloc_i_bin_header_t* node )
{
    node->next = head->first;
    head->first = node;
}

// Get the head of a page list 
inline static malloc_i_bin_header_t* __attribute__((always_inline)) malloc_i_list_head( malloc_i_bin_header_head_t* head )
{
    return head->first;
}

// Skip list 
// XOR shift rng 
static uint32_t __attribute__((pure)) malloc_i_skip_rand( void )
{
    static uint32_t x = 123456789;
    static uint32_t y = 362436069;
    static uint32_t z = 521288629;
    static uint32_t w = 88675123;

    uint32_t t;

    t = x ^ (x << 11);
    x = y; y = z; z = w;
    return w = w ^ (w >> 19) ^ t ^ (t >> 8);
}

// Generate a random level for a skip node 
inline static int __attribute__((pure, always_inline)) malloc_i_random_level( void )
{
    int level = 0;
    while( malloc_i_skip_rand() < SKIP_P && level < SKIP_MAX_LEVEL )
    {
        ++level;
    }

    return level;
}

// Find best fit for a given value 
static malloc_i_big_bin_header_t* malloc_i_skip_list_findbest( uintptr_t search_size )
{
    malloc_i_big_bin_header_t* node = &malloc_i_big_bins.head;
    for( int i = malloc_i_big_bins.level; i >= 0; --i )
    {
        while( node->forward[i] && (node->forward[i]->size < search_size) )
        {
            node = node->forward[i];
            if( node )
                if( (node->size + sizeof(malloc_i_big_bin_header_t)) % PAGE_SIZE != 0 ) KPANIC("MALLOC ERROR", NULL);
        }
    }

    node = node->forward[0];
    if( node )
    {
        if( (uintptr_t)node % PAGE_SIZE != 0 ) KPANIC("MALLOC ERROR", NULL);
        if( (node->size + sizeof(malloc_i_big_bin_header_t)) % PAGE_SIZE != 0 ) KPANIC("MALLOC ERROR", NULL);
    }
    return node;
}

// Insert a header into the skip list 
static void malloc_i_skip_list_insert( malloc_i_big_bin_header_t* value )
{
    malloc_i_big_bin_header_t* node = &malloc_i_big_bins.head;
    malloc_i_big_bin_header_t* update[SKIP_MAX_LEVEL + 1];

    for( int i = malloc_i_big_bins.level; i >= 0; --i )
    {
        while( node->forward[i] && node->forward[i]->size < value->size )
        {
            node = node->forward[i];
            if( (node->size + sizeof(malloc_i_big_bin_header_t)) % PAGE_SIZE != 0 ) KPANIC("MALLOC ERROR", NULL);
        }
        update[i] = node;
    }
    node = node->forward[0];

    if( node != value )
    {
        int level = malloc_i_random_level();

        if( level > malloc_i_big_bins.level )
        {
            for( int i = malloc_i_big_bins.level + 1; i <= level; ++i )
            {
                update[i] = &malloc_i_big_bins.head;
            }
            malloc_i_big_bins.level = level;
        }

        node = value;

        for( int i = 0; i <= level; ++i )
        {
            node->forward[i] = update[i]->forward[i];
            if( node->forward[i] )
            {
                if( (node->forward[i]->size + sizeof(malloc_i_big_bin_header_t)) % PAGE_SIZE != 0 ) KPANIC("MALLOC ERROR", NULL);
            }
            update[i]->forward[i] = node;
        }
    }
}

// Delete a header from the skip list 
static void malloc_i_skip_list_delete( malloc_i_big_bin_header_t* value )
{
    malloc_i_big_bin_header_t* node = &malloc_i_big_bins.head;
    malloc_i_big_bin_header_t* update[SKIP_MAX_LEVEL + 1];

    for( int i = malloc_i_big_bins.level; i >= 0; --i )
    {
        while( node->forward[i] && node->forward[i]->size < value->size )
        {
            node = node->forward[i];
            if( node )
            {
                if( (node->size + sizeof(malloc_i_big_bin_header_t)) % PAGE_SIZE != 0 ) KPANIC("MALLOC ERROR", NULL);
            }
        }
        update[i] = node;
    }
    node = node->forward[0];
    while( node != value )
    {
        node = node->forward[0];
    }

    if( node != value )
    {
        node = malloc_i_big_bins.head.forward[0];
        while( node->forward[0] && node->forward[0] != value )
        {
            node = node->forward[0];
        }
        node = node->forward[0];
    }

    if( node == value )
    {
        for( int i = 0; i <= malloc_i_big_bins.level; i++ )
        {
            if( update[i]->forward[i] != node )
            {
                break;
            }
            update[i]->forward[i] = node->forward[i];
            if( update[i]->forward[i] )
            {
                if( (uintptr_t)(update[i]->forward[i]) % PAGE_SIZE != 0 ) KPANIC("MALLOC ERROR", NULL);
                if( (uintptr_t)(update[i]->forward[i]->size + sizeof(malloc_i_big_bin_header_t)) % PAGE_SIZE != 0 ) KPANIC("MALLOC ERROR", NULL);
            }
        }

        while( malloc_i_big_bins.level > 0 && malloc_i_big_bins.head.forward[malloc_i_big_bins.level] == NULL )
        {
            --malloc_i_big_bins.level;
        }
    }
}

// Stack 
// Pop an item from a block. 
static void* malloc_i_stack_pop( malloc_i_bin_header_t* header )
{
    void* item = header->head;
    uintptr_t** head = header->head;
    uintptr_t* next = *head;
    header->head = next;
    return item;
}

// Push an item into a block 
static void malloc_i_stack_push( malloc_i_bin_header_t* header, void* ptr )
{
    uintptr_t** item = (uintptr_t**)ptr;
    *item = (uintptr_t*)header->head;
    header->head = item;
}

// Is this cell stack empty 
inline static int __attribute__((always_inline)) malloc_i_stack_empty( malloc_i_bin_header_t* header )
{
    return header->head == NULL;
}

// malloc() 
static void* __attribute__((malloc)) malloc_i( uintptr_t size )
{
    if( __builtin_expect(size == 0, 0) )
    {
        return NULL;
    }

    unsigned int bucket_id = malloc_i_bin_size(size);

    if( bucket_id < BIG_BIN )
    {
        // Small Bins 
        malloc_i_bin_header_t* bin_header = malloc_i_list_head(&malloc_i_bin_head[bucket_id]);
        if( !bin_header )
        {
            // Grow the heap for the new bin 
            bin_header = (malloc_i_bin_header_t*)sbrk(PAGE_SIZE);
            bin_header->bin_magic = BIN_MAGIC;

            bin_header->head = (void*)((uintptr_t)bin_header + sizeof(malloc_i_bin_header_t));

            malloc_i_list_insert(&malloc_i_bin_head[bucket_id], bin_header);

            uintptr_t adj = SMALLEST_BIN_LOG + bucket_id;
            uintptr_t available = ((PAGE_SIZE - sizeof(malloc_i_bin_header_t)) >> adj) - 1;

            uintptr_t** base = bin_header->head;
            for( uintptr_t i = 0; i < available; ++i )
            {
                base[i << bucket_id] = (uintptr_t*)&base[(i + 1) << bucket_id];
            }

            base[available << bucket_id] = NULL;
            bin_header->size = bucket_id;
        }
        uintptr_t** item = malloc_i_stack_pop(bin_header);
        if( malloc_i_stack_empty(bin_header) )
        {
            malloc_i_list_decouple(&(malloc_i_bin_head[bucket_id]), bin_header);
        }
        return item;
    }else
    {
        // Big bins 
        malloc_i_big_bin_header_t* bin_header = malloc_i_skip_list_findbest(size);
        if( bin_header )
        {
            malloc_i_skip_list_delete(bin_header);
            uintptr_t** item = malloc_i_stack_pop((malloc_i_bin_header_t*)bin_header);

            return item;
        }else
        {
            uintptr_t pages = (size + sizeof(malloc_i_big_bin_header_t)) / PAGE_SIZE + 1;
            bin_header = (malloc_i_big_bin_header_t*)sbrk(PAGE_SIZE * pages);
            bin_header->bin_magic = BIN_MAGIC;

            bin_header->size = pages * PAGE_SIZE - sizeof(malloc_i_big_bin_header_t);

            bin_header->prev = malloc_i_newest_big;
            if( bin_header->prev )
            {
                bin_header->prev->next = bin_header;
            }
            malloc_i_newest_big = bin_header;
            bin_header->next = NULL;

            bin_header->head = NULL;
            return (void*)((uintptr_t)bin_header + sizeof(malloc_i_big_bin_header_t));
        }
    }
}

// free() 
static void free_i( void* ptr )
{
    if( __builtin_expect(ptr == NULL, 0 ) )
    {
        return;
    }

    if( (uintptr_t)ptr % PAGE_SIZE == 0 )
    {
        ptr = (void*)((uintptr_t)ptr - 1);
    }

    malloc_i_bin_header_t* header = (malloc_i_bin_header_t*)((uintptr_t)ptr & (uintptr_t)~PAGE_MASK);

    if( header->bin_magic != BIN_MAGIC )
        return;

    uintptr_t bucket_id = header->size;
    if( bucket_id > (uintptr_t)NUM_BINS )
    {
        bucket_id = BIG_BIN;
        malloc_i_big_bin_header_t* bheader = (malloc_i_big_bin_header_t*)header;

        malloc_i_stack_push((malloc_i_bin_header_t*)bheader, (void*)((uintptr_t)bheader + sizeof(malloc_i_big_bin_header_t)));

        malloc_i_skip_list_insert(bheader);
    }else
    {
        if( malloc_i_stack_empty(header) )
        {
            malloc_i_list_insert(&malloc_i_bin_head[bucket_id], header);
        }

        malloc_i_stack_push(header, ptr);
    }
}

// valloc() 
static void* __attribute__((malloc)) valloc_i( uintptr_t size )
{
    uintptr_t true_size = size + PAGE_SIZE - sizeof(malloc_i_big_bin_header_t);
    void* result = malloc_i(true_size);
    void* out = (void*)((uintptr_t)result + (PAGE_SIZE - sizeof(malloc_i_big_bin_header_t)));

    return out;
}

// realloc() 
static void* __attribute__((malloc)) realloc_i( void* ptr, uintptr_t size )
{
    if( __builtin_expect(ptr == NULL, 0) )
    {
        return malloc_i(size);
    }

    if( __builtin_expect(size == 0, 0) )
    {
        free_i(ptr);
        return NULL;
    }

    malloc_i_bin_header_t* header_old = (void*)((uintptr_t)ptr & (uintptr_t)~PAGE_MASK);
    if( header_old->bin_magic != BIN_MAGIC )
    {
        return NULL;
    }

    uintptr_t old_size = header_old->size;
    if( old_size < (uintptr_t)BIG_BIN )
    {
        old_size = (1UL << (SMALLEST_BIN_LOG + old_size));
    }

    if( old_size >= size )
    {
        return ptr;
    }

    // reallocate more memory 
    void* newptr = malloc_i(size);
    if( __builtin_expect(newptr != NULL, 1) )
    {
        memcpy(newptr, ptr, old_size);
        free_i(ptr);
        return newptr;
    }

    return NULL;
}

static void* __attribute__((malloc)) calloc_i( uintptr_t nmemb, uintptr_t size )
{
    void* ptr = malloc_i(nmemb * size);
    if( __builtin_expect(ptr != NULL, 1) )
    {
        memset(ptr, 0x00, nmemb * size);
    }

    return ptr;
}
