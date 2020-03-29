#include <stdlib.h>
#include <string.h>
#include <kernel/bitset.h>

#define CEIL(NUMBER, BASE) ((((NUMBER) + BASE) - 1) & ~( (BASE) - 1 ))
#define iom \
    size_t index = bit >> 3; \
    bit = bit - index * 8; \
    size_t offset = bit & 7; \
    size_t mask = 1 << offset;

// Initialize given bitset_t of given size at *set
void bitset_init( bitset_t* set, size_t size )
{
    set->size = CEIL(size, 8);
    set->data = malloc(set->size);
    memset(set->data, 0, set->size);
}

// Free data in bitset_t *set
void bitset_free( bitset_t* set )
{
    free(set->data);
}

// Reallocate bitset_t *set to be size
static void bitset_resize( bitset_t* set, size_t size )
{
    if( set->size >= size )
    {
        return;
    }

    set->data = realloc(set->data, size);
    memset(set->data + set->size, 0, size - set->size);
    set->size = size;
}

// Set bit of bitset_t *set
void bitset_set( bitset_t* set, size_t bit )
{
    iom;
    if( set->size <= index )
    {
        bitset_resize( set, set->size << 1 );
    }
    set->data[index] = (unsigned char)(set->data[index] | mask);
}

// Clear bitset_t *set
void bitset_clear( bitset_t* set, size_t bit )
{
    iom;
    set->data[index] = (unsigned char)(set->data[index] & ~mask);
}

// Test bit of bitset_t *set
int bitset_test( bitset_t* set, size_t bit )
{
    iom;
    return !!(mask & set->data[index]);
}

// Find first unset bit of bitset_t *set
int bitset_ffub( bitset_t* set )
{
    for( size_t i = 0; i < set->size * 8; i++ )
    {
        if( bitset_test(set, i) )
        {
            continue;
        }
        return (int)i;
    }
    return -1;
}
