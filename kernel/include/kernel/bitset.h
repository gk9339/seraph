#ifndef _KERNEL_BITSET_H
#define _KERNEL_BITSET_H

#include <stddef.h> // size_t

typedef struct
{
    unsigned char* data; // Actual bit array
    size_t size; // Size of bitset, multiple of 8
} bitset_t;

void bitset_init( bitset_t* set, size_t size ); // Initialize given bitset_t of given size at *set
void bitset_free(bitset_t *set); // Free data in bitset_t *set
void bitset_set(bitset_t *set, size_t bit); // Set bit of bitset_t *set
void bitset_clear(bitset_t *set, size_t bit); // Clear bitset_t *set
int bitset_test(bitset_t *set, size_t bit); // Test bit of bitset_t *set
int bitset_ffub(bitset_t *set); // Find first unset bit in bitset_t *set

#endif
