#ifndef _LIST_H
#define _LIST_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stddef.h>

typedef struct node
{
    struct node* next;
    struct node* prev;
    void* value;
    void* owner;
} __attribute__((packed)) node_t;

typedef struct
{
    node_t* head;
    node_t* tail;
    size_t length;
} __attribute__((packed)) list_t;

list_t* list_create( void );
node_t* list_insert( list_t* list, void* item );
void list_destroy( list_t* list );
void list_free( list_t* list );
void list_append( list_t* list, node_t* item );
node_t* list_find( list_t* list, void* value );
size_t list_index_of( list_t* list, void* value );
void list_remove( list_t* list, size_t index );
void list_delete( list_t* list, node_t* node );
node_t* list_pop( list_t* list );
node_t* list_dequeue( list_t* list );
list_t* list_copy( list_t* list );
void list_merge( list_t* target, list_t* source );

void list_append_after( list_t* list, node_t* before, node_t* node );
node_t* list_insert_after( list_t* list, node_t* before, void* item );

void list_append_before( list_t* list, node_t* after, node_t* node );
node_t* list_insert_before( list_t* list, node_t* after, void* item );

#define foreach(i, list) for( node_t* i = (list)->head; i != NULL; i = i->next )
#define foreachr(i, list) for( node_t* i = (list)->tail; i != NULL; i = i->prev )

#ifdef __cplusplus
}
#endif

#endif
