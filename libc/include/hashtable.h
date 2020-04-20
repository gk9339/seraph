#ifndef _HASHTABLE_H
#define _HASHTABLE_H

#ifdef __cplusplus
extern "C" {
#endif

#include <list.h>

typedef unsigned int (*hashtable_hash_t)(void* key);
typedef int (*hashtable_comp_t)(void* a, void* b);
typedef void (*hashtable_free_t)(void*);
typedef void* (*hashtable_dupe_t)(void*);

typedef struct hashtable_entry
{
    char* key;
    void* value;
    struct hashtable_entry* next;
} hashtable_entry_t;

typedef struct hashtable
{
    hashtable_hash_t hash_func;
    hashtable_comp_t hash_comp;
    hashtable_dupe_t hash_key_dup;
    hashtable_free_t hash_key_free;
    hashtable_free_t hash_val_free;
    size_t size;
    hashtable_entry_t** entries;
} hashtable_t;

hashtable_t* hashtable_create( int size );
hashtable_t* hashtable_create_int( int size );
void* hashtable_set( hashtable_t* hashtab, void* key, void* value );
void* hashtable_get( hashtable_t* hashtab, void* key );
void* hashtable_remove( hashtable_t* hashtab, void* key );
int hashtable_has( hashtable_t* hashtab, void* key );
list_t* hashtable_keys( hashtable_t* hashtab );
list_t* hashtable_values( hashtable_t* hashtab );
void hashtable_free( hashtable_t* hashtab );

unsigned int hashtable_string_hash( void* key );
int hashtable_string_comp( void* a, void* b );
void* hashtable_string_dupe( void* key );
int hashtable_is_empty( hashtable_t* hashtab );

#ifdef __cplusplus
}
#endif

#endif
