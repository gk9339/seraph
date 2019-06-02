#include <stdlib.h>
#include <string.h>
#include <limits.h>
#ifdef __is_libk
#include <kernel/types.h>
#include <kernel/kernel.h>
#include <kernel/mem.h>
#include <kernel/spinlock.h>
#else
#include <sys/syscall.h>
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

/* Internal functions */
#if defined(__is_libk)
static void* __attribute__((malloc)) kmalloc_i( uintptr_t size );
static void* __attribute__((malloc)) krealloc_i( void* ptr, uintptr_t size );
static void* __attribute__((malloc)) kcalloc_i( uintptr_t nmemb, uintptr_t size );
static void* __attribute__((malloc)) kvalloc_i( uintptr_t size );
static void kfree_i( void* ptr );
#else
typedef volatile int spin_lock_t[2];

static void* __attribute__((malloc)) malloc_i( uintptr_t size );
static void* __attribute__((malloc)) realloc_i( void* ptr, uintptr_t size );
static void* __attribute__((malloc)) calloc_i( uintptr_t nmemb, uintptr_t size );
static void* __attribute__((malloc)) valloc_i( uintptr_t size );
static void free_i( void* ptr );

static void spin_lock( volatile int* lock )
{
    while( __sync_lock_test_and_set(lock, 0x01) )
    {
        syscall_yield();
    }
}

static void spin_unlock( volatile int* lock )
{
    __sync_lock_release(lock);
}
#endif

static spin_lock_t mem_lock = { 0 };

void* __attribute__((malloc)) malloc( uintptr_t size )
{
    spin_lock(mem_lock);
#if defined(__is_libk)
    void* ret = kmalloc_i(size);
#else
	// TODO: userland malloc()
    void* ret = NULL;
#endif
    spin_unlock(mem_lock);
    return ret;
}

void* __attribute__((malloc)) realloc( void* ptr, uintptr_t size )
{
    spin_lock(mem_lock);
#if defined(__is_libk)
    void* ret = krealloc_i(ptr, size);
#else
	// TODO: userland realloc()
    void* ret = NULL;
#endif
    spin_unlock(mem_lock);
    return ret;
}

void* __attribute__((malloc)) calloc( uintptr_t nmemb, uintptr_t size )
{
    spin_lock(mem_lock);
#if defined(__is_libk)
    void* ret = kcalloc_i(nmemb, size);
#else
	// TODO: userland calloc()
    void* ret = NULL;
#endif
    spin_unlock(mem_lock);
    return ret;
}

void* __attribute__((malloc)) valloc( uintptr_t size )
{
    spin_lock(mem_lock);
#if defined(__is_libk)
    void* ret = kvalloc_i(size);
#else
	// TODO: userland valloc()
    void* ret = NULL;
#endif
    spin_unlock(mem_lock);
    return ret;
}

void free( void* ptr )
{
    spin_lock(mem_lock);
#if defined(__is_libk)
        kfree_i(ptr);
#else
    // TODO: userland free()
#endif
    spin_unlock(mem_lock);
}

#if defined(__is_libk)
/* Bin management */
/* Adjust bin size in bin_size call to proper bounds */
inline static uintptr_t __attribute__((always_inline, pure)) kmalloc_i_adjust_bin( uintptr_t bin )
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

/* Given a size value, find the correct bin to place the allocation in */
inline static uintptr_t __attribute__((always_inline, pure)) kmalloc_i_bin_size( uintptr_t size )
{
    uintptr_t bin = sizeof(size) * CHAR_BIT - __builtin_clzl(size);
    bin += !!(size & (size -1 ));
    
    return kmalloc_i_adjust_bin(bin);
}

/* Bin header */
typedef struct _kmalloc_i_bin_header
{
    struct _kmalloc_i_bin_header* next;
    void* head;
    uintptr_t size;
    uint32_t bin_magic;
} kmalloc_i_bin_header_t;

/* Big bin header, different pointers */
typedef struct _kmalloc_i_big_bin_header
{
    struct _kmalloc_i_big_bin_header* next;
    void* head;
    uintptr_t size;
    uint32_t bin_magic;
    struct _kmalloc_i_big_bin_header* prev;
    struct _kmalloc_i_big_bin_header* forward[SKIP_MAX_LEVEL+1];
} kmalloc_i_big_bin_header_t;

/* List of pages in a bin */
typedef struct _kmalloc_i_bin_header_head
{
    kmalloc_i_bin_header_t* first;
} kmalloc_i_bin_header_head_t;

/* Array of available bins */
static kmalloc_i_bin_header_head_t kmalloc_i_bin_head[NUM_BINS - 1];
static struct _kmalloc_i_big_bins
{
    kmalloc_i_big_bin_header_t head;
    int level;
} kmalloc_i_big_bins;
static kmalloc_i_big_bin_header_t* kmalloc_i_newest_big = NULL;

/* Doubly-linked list */
/* Remove an entry from a page list */
inline static void __attribute__((always_inline)) kmalloc_i_list_decouple( kmalloc_i_bin_header_head_t* head, kmalloc_i_bin_header_t* node )
{
    kmalloc_i_bin_header_t* next = node->next;
    head->first = next;
    node->next = NULL;
}

/* Insert an entry into a page list */
inline static void __attribute__((always_inline)) kmalloc_i_list_insert( kmalloc_i_bin_header_head_t* head, kmalloc_i_bin_header_t* node )
{
    node->next = head->first;
    head->first = node;
}

/* Get the head of a page list */
inline static kmalloc_i_bin_header_t* __attribute__((always_inline)) kmalloc_i_list_head( kmalloc_i_bin_header_head_t* head )
{
    return head->first;
}

/* Skip list */
/* XOR shift rng */
static uint32_t __attribute__((pure)) kmalloc_i_skip_rand( void )
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

/* Generate a random level for a skip node */
inline static int __attribute__((pure, always_inline)) kmalloc_i_random_level( void )
{
    int level = 0;
    while( kmalloc_i_skip_rand() < SKIP_P && level < SKIP_MAX_LEVEL )
    {
        ++level;
    }

    return level;
}

/* Find best fit for a given value */
static kmalloc_i_big_bin_header_t* kmalloc_i_skip_list_findbest( uintptr_t search_size )
{
    kmalloc_i_big_bin_header_t* node = &kmalloc_i_big_bins.head;
    for( int i = kmalloc_i_big_bins.level; i >= 0; --i )
    {
        while( node->forward[i] && (node->forward[i]->size < search_size) )
        {
            node = node->forward[i];
            if( node )
                if( (node->size + sizeof(kmalloc_i_big_bin_header_t)) % PAGE_SIZE != 0 ) KPANIC("KMALLOC ERROR", NULL);
        }
    }

    node = node->forward[0];
    if( node )
    {
        if( (uintptr_t)node % PAGE_SIZE != 0 ) KPANIC("KMALLOC ERROR", NULL);
        if( (node->size + sizeof(kmalloc_i_big_bin_header_t)) % PAGE_SIZE != 0 ) KPANIC("KMALLOC ERROR", NULL);
    }
    return node;
}

/* Insert a header into the skip list */
static void kmalloc_i_skip_list_insert( kmalloc_i_big_bin_header_t* value )
{
    kmalloc_i_big_bin_header_t* node = &kmalloc_i_big_bins.head;
    kmalloc_i_big_bin_header_t* update[SKIP_MAX_LEVEL + 1];

    for( int i = kmalloc_i_big_bins.level; i >= 0; --i )
    {
        while( node->forward[i] && node->forward[i]->size < value->size )
        {
            node = node->forward[i];
            if( (node->size + sizeof(kmalloc_i_big_bin_header_t)) % PAGE_SIZE != 0 ) KPANIC("KMALLOC ERROR", NULL);
        }
        update[i] = node;
    }
    node = node->forward[0];

    if( node != value )
    {
        int level = kmalloc_i_random_level();

        if( level > kmalloc_i_big_bins.level )
        {
            for( int i = kmalloc_i_big_bins.level + 1; i <= level; ++i )
            {
                update[i] = &kmalloc_i_big_bins.head;
            }
            kmalloc_i_big_bins.level = level;
        }

        node = value;

        for( int i = 0; i <= level; ++i )
        {
            node->forward[i] = update[i]->forward[i];
            if( node->forward[i] )
            {
                if( (node->forward[i]->size + sizeof(kmalloc_i_big_bin_header_t)) % PAGE_SIZE != 0 ) KPANIC("KMALLOC ERROR", NULL);
            }
            update[i]->forward[i] = node;
        }
    }
}

/* Delete a header from the skip list */
static void kmalloc_i_skip_list_delete( kmalloc_i_big_bin_header_t* value )
{
    kmalloc_i_big_bin_header_t* node = &kmalloc_i_big_bins.head;
    kmalloc_i_big_bin_header_t* update[SKIP_MAX_LEVEL + 1];

    for( int i = kmalloc_i_big_bins.level; i >= 0; --i )
    {
        while( node->forward[i] && node->forward[i]->size < value->size )
        {
            node = node->forward[i];
            if( node )
            {
                if( (node->size + sizeof(kmalloc_i_big_bin_header_t)) % PAGE_SIZE != 0 ) KPANIC("KMALLOC ERROR", NULL);
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
        node = kmalloc_i_big_bins.head.forward[0];
        while( node->forward[0] && node->forward[0] != value )
        {
            node = node->forward[0];
        }
        node = node->forward[0];
    }

    if( node == value )
    {
        for( int i = 0; i <= kmalloc_i_big_bins.level; i++ )
        {
            if( update[i]->forward[i] != node )
            {
                break;
            }
            update[i]->forward[i] = node->forward[i];
            if( update[i]->forward[i] )
            {
                if( (uintptr_t)(update[i]->forward[i]) % PAGE_SIZE != 0 ) KPANIC("KMALLOC ERROR", NULL);
                if( (uintptr_t)(update[i]->forward[i] + sizeof(kmalloc_i_big_bin_header_t)) % PAGE_SIZE != 0 ) KPANIC("KMALLOC ERROR", NULL);
            }
        }

        while( kmalloc_i_big_bins.level > 0 && kmalloc_i_big_bins.head.forward[kmalloc_i_big_bins.level] == NULL )
        {
            --kmalloc_i_big_bins.level;
        }
    }
}

/* Stack */
/* Pop an item from a block. */
static void* kmalloc_i_stack_pop( kmalloc_i_bin_header_t* header )
{
    void* item = header->head;
    uintptr_t** head = header->head;
    uintptr_t* next = *head;
    header->head = next;
    return item;
}

/* Push an item into a block */
static void kmalloc_i_stack_push( kmalloc_i_bin_header_t* header, void* ptr )
{
    uintptr_t** item = (uintptr_t**)ptr;
    *item = (uintptr_t*)header->head;
    header->head = item;
}

/* Is this cell stack empty */
inline static int __attribute__((always_inline)) kmalloc_i_stack_empty( kmalloc_i_bin_header_t* header )
{
    return header->head == NULL;
}

/* malloc() */
static void* __attribute__((malloc)) kmalloc_i( uintptr_t size )
{
    if( __builtin_expect(size == 0, 0) )
    {
        return NULL;
    }

    unsigned int bucket_id = kmalloc_i_bin_size(size);

    if( bucket_id < BIG_BIN )
    {
        /* Small Bins */
        kmalloc_i_bin_header_t* bin_header = kmalloc_i_list_head(&kmalloc_i_bin_head[bucket_id]);
        if( !bin_header )
        {
            /* Grow the heap for the new bin */
            bin_header = (kmalloc_i_bin_header_t*)sbrk(PAGE_SIZE);
            bin_header->bin_magic = BIN_MAGIC;

            bin_header->head = (void*)((uintptr_t)bin_header + sizeof(kmalloc_i_bin_header_t));

            kmalloc_i_list_insert(&kmalloc_i_bin_head[bucket_id], bin_header);

            uintptr_t adj = SMALLEST_BIN_LOG + bucket_id;
            uintptr_t available = ((PAGE_SIZE - sizeof(kmalloc_i_bin_header_t)) >> adj) - 1;

            uintptr_t** base = bin_header->head;
            for( uintptr_t i = 0; i < available; ++i )
            {
                base[i << bucket_id] = (uintptr_t*)&base[(i + 1) << bucket_id];
            }

            base[available << bucket_id] = NULL;
            bin_header->size = bucket_id;
        }
        uintptr_t** item = kmalloc_i_stack_pop(bin_header);
        if( kmalloc_i_stack_empty(bin_header) )
        {
            kmalloc_i_list_decouple(&(kmalloc_i_bin_head[bucket_id]), bin_header);
        }
        return item;
    }else
    {
        /* Big bins */
        kmalloc_i_big_bin_header_t* bin_header = kmalloc_i_skip_list_findbest(size);
        if( bin_header )
        {
            kmalloc_i_skip_list_delete(bin_header);
            uintptr_t** item = kmalloc_i_stack_pop((kmalloc_i_bin_header_t*)bin_header);

            return item;
        }else
        {
            uintptr_t pages = (size + sizeof(kmalloc_i_big_bin_header_t)) / PAGE_SIZE + 1;
            bin_header = (kmalloc_i_big_bin_header_t*)sbrk(PAGE_SIZE * pages);
            bin_header->bin_magic = BIN_MAGIC;

            bin_header->size = pages * PAGE_SIZE - sizeof(kmalloc_i_big_bin_header_t);

            bin_header->prev = kmalloc_i_newest_big;
            if( bin_header->prev )
            {
                bin_header->prev->next = bin_header;
            }
            kmalloc_i_newest_big = bin_header;
            bin_header->next = NULL;

            bin_header->head = NULL;
            return (void*)((uintptr_t)bin_header + sizeof(kmalloc_i_big_bin_header_t));
        }
    }
}

/* free() */
static void kfree_i( void* ptr )
{
    if( __builtin_expect(ptr == NULL, 0 ) )
    {
        return;
    }

    if( (uintptr_t)ptr % PAGE_SIZE == 0 )
    {
        ptr = (void*)((uintptr_t)ptr - 1);
    }

    kmalloc_i_bin_header_t* header = (kmalloc_i_bin_header_t*)((uintptr_t)ptr & (uintptr_t)~PAGE_MASK);

    if( header->bin_magic != BIN_MAGIC )
        return;

    uintptr_t bucket_id = header->size;
    if( bucket_id > (uintptr_t)NUM_BINS )
    {
        bucket_id = BIG_BIN;
        kmalloc_i_big_bin_header_t* bheader = (kmalloc_i_big_bin_header_t*)header;

        kmalloc_i_stack_push((kmalloc_i_bin_header_t*)bheader, (void*)((uintptr_t)bheader + sizeof(kmalloc_i_big_bin_header_t)));

        kmalloc_i_skip_list_insert(bheader);
    }else
    {
        if( kmalloc_i_stack_empty(header) )
        {
            kmalloc_i_list_insert(&kmalloc_i_bin_head[bucket_id], header);
        }

        kmalloc_i_stack_push(header, ptr);
    }
}

/* valloc() */
static void* __attribute__((malloc)) kvalloc_i( uintptr_t size )
{
    uintptr_t true_size = size + PAGE_SIZE - sizeof(kmalloc_i_big_bin_header_t);
    void* result = kmalloc_i(true_size);
    void* out = (void*)((uintptr_t)result + (PAGE_SIZE - sizeof(kmalloc_i_big_bin_header_t)));

    return out;
}

/* realloc() */
static void* __attribute__((malloc)) krealloc_i( void* ptr, uintptr_t size )
{
    if( __builtin_expect(ptr == NULL, 0) )
    {
        return NULL;
    }

    if( __builtin_expect(size == 0, 0) )
    {
        free(ptr);
        return NULL;
    }

    kmalloc_i_bin_header_t* header_old = (void*)((uintptr_t)ptr & (uintptr_t)~PAGE_MASK);
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

    /* reallocate more memory */
    void* newptr = kmalloc_i(size);
    if( __builtin_expect(newptr != NULL, 1) )
    {
        memcpy(newptr, ptr, old_size);
        kfree_i(ptr);
        return newptr;
    }

    return NULL;
}

static void* __attribute__((malloc)) kcalloc_i( uintptr_t nmemb, uintptr_t size )
{
    void* ptr = kmalloc_i(nmemb * size);
    if( __builtin_expect(ptr != NULL, 1) )
    {
        memset(ptr, 0x00, nmemb * size);
    }

    return ptr;
}

#else
    // TODO: userland internals
#endif
