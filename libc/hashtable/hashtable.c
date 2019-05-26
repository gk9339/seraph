#include <list.h>
#include <hashtable.h>
#include <stdlib.h>
#include <string.h>

unsigned int hashtable_string_hash(void * _key) 
{
    unsigned int hash = 0;
    char * key = (char *)_key;
    int c;
    /* This is the so-called "sdbm" hash. It comes from a piece of
     * public domain code from a clone of ndbm. */
    while( (c = *key++) ) 
    {
        hash = c + (hash << 6) + (hash << 16) - hash;
    }
    return hash;
}

int hashtable_string_comp(void * a, void * b) 
{
    return !strcmp(a,b);
}

void * hashtable_string_dupe(void * key) 
{
    return strdup(key);
}

unsigned int hashtable_int_hash(void * key) 
{
    return (unsigned int)key;
}

int hashtable_int_comp(void * a, void * b) 
{
    return (int)a == (int)b;
}

void * hashtable_int_dupe(void * key) 
{
    return key;
}

static void hashtable_int_free(void * ptr) 
{
    (void)ptr;
    return;
}


hashtable_t * hashtable_create(int size) \
{
    hashtable_t * table = malloc(sizeof(hashtable_t));

    table->hash_func     = &hashtable_string_hash;
    table->hash_comp     = &hashtable_string_comp;
    table->hash_key_dup  = &hashtable_string_dupe;
    table->hash_key_free = &free;
    table->hash_val_free = &free;

    table->size = size;
    table->entries = malloc(sizeof(hashtable_entry_t *) * size);
    memset(table->entries, 0x00, sizeof(hashtable_entry_t *) * size);

    return table;
}

hashtable_t * hashtable_create_int(int size) 
{
    hashtable_t * table = malloc(sizeof(hashtable_t));

    table->hash_func     = &hashtable_int_hash;
    table->hash_comp     = &hashtable_int_comp;
    table->hash_key_dup  = &hashtable_int_dupe;
    table->hash_key_free = &hashtable_int_free;
    table->hash_val_free = &free;

    table->size = size;
    table->entries = malloc(sizeof(hashtable_entry_t *) * size);
    memset(table->entries, 0x00, sizeof(hashtable_entry_t *) * size);

    return table;
}

void * hashtable_set(hashtable_t * table, void * key, void * value) 
{
    unsigned int hash = table->hash_func(key) % table->size;

    hashtable_entry_t * x = table->entries[hash];
    if( !x ) 
    {
        hashtable_entry_t * e = malloc(sizeof(hashtable_entry_t));
        e->key   = table->hash_key_dup(key);
        e->value = value;
        e->next = NULL;
        table->entries[hash] = e;
        return NULL;
    } else 
    {
        hashtable_entry_t * p = NULL;
        do {
            if( table->hash_comp(x->key, key) ) 
            {
                void * out = x->value;
                x->value = value;
                return out;
            } else 
            {
                p = x;
                x = x->next;
            }
        }while( x );
        hashtable_entry_t * e = malloc(sizeof(hashtable_entry_t));
        e->key   = table->hash_key_dup(key);
        e->value = value;
        e->next = NULL;

        p->next = e;
        return NULL;
    }
}

void * hashtable_get(hashtable_t * table, void * key) 
{
    unsigned int hash = table->hash_func(key) % table->size;

    hashtable_entry_t * x = table->entries[hash];
    if( !x ) 
    {
        return NULL;
    } else 
    {
        do {
            if( table->hash_comp(x->key, key) ) 
            {
                return x->value;
            }
            x = x->next;
        }while( x );
        return NULL;
    }
}

void * hashtable_remove(hashtable_t * table, void * key) 
{
    unsigned int hash = table->hash_func(key) % table->size;

    hashtable_entry_t * x = table->entries[hash];
    if( !x ) 
    {
        return NULL;
    } else 
    {
        if( table->hash_comp(x->key, key) ) 
        {
            void * out = x->value;
            table->entries[hash] = x->next;
            table->hash_key_free(x->key);
            table->hash_val_free(x);
            return out;
        } else 
        {
            hashtable_entry_t * p = x;
            x = x->next;
            do {
                if( table->hash_comp(x->key, key) ) 
                {
                    void * out = x->value;
                    p->next = x->next;
                    table->hash_key_free(x->key);
                    table->hash_val_free(x);
                    return out;
                }
                p = x;
                x = x->next;
            }while( x );
        }
        return NULL;
    }
}

int hashtable_has(hashtable_t * table, void * key) 
{
    unsigned int hash = table->hash_func(key) % table->size;

    hashtable_entry_t * x = table->entries[hash];
    if( !x ) 
    {
        return 0;
    } else 
    {
        do {
            if( table->hash_comp(x->key, key) ) 
            {
                return 1;
            }
            x = x->next;
        }while( x );
        return 0;
    }

}

list_t * hashtable_keys(hashtable_t * table) 
{
    list_t * l = list_create();

    for( unsigned int i = 0; i < table->size; ++i )
    {
        hashtable_entry_t * x = table->entries[i];
        while( x )
        {
            list_insert(l, x->key);
            x = x->next;
        }
    }

    return l;
}

list_t * hashtable_values(hashtable_t * table) 
{
    list_t * l = list_create();

    for( unsigned int i = 0; i < table->size; ++i )
    {
        hashtable_entry_t * x = table->entries[i];
        while( x )
        {
            list_insert(l, x->value);
            x = x->next;
        }
    }

    return l;
}

void hashtable_free(hashtable_t * table) 
{
    for( unsigned int i = 0; i < table->size; ++i )
    {
        hashtable_entry_t * x = table->entries[i], * p;
        while( x )
        {
            p = x;
            x = x->next;
            table->hash_key_free(p->key);
            table->hash_val_free(p);
        }
    }
    free(table->entries);
}

int hashtable_is_empty(hashtable_t * table)
{
    for( unsigned int i = 0; i < table->size; ++i )
    {
        if (table->entries[i]) return 0;
    }
    return 1;
}
