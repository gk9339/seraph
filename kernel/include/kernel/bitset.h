#ifndef _KERNEL_BITSET_H
#define _KERNEL_BITSET_H

#include <stddef.h> /* for size_t */

typedef struct
{
    unsigned char* data;
    size_t size;
} bitset_t;

void bitset_init( bitset_t* set, size_t size );
void bitset_free(bitset_t *set);
void bitset_set(bitset_t *set, size_t bit);
void bitset_clear(bitset_t *set, size_t bit);
int bitset_test(bitset_t *set, size_t bit);
int bitset_ffub(bitset_t *set); /* Find first unset bit */

#endif
