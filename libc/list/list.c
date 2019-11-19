#include <list.h>
#include <stdlib.h>
#include <stddef.h>

list_t* list_create( void )
{
    list_t* list = malloc(sizeof(list_t));
    list->head = NULL;
    list->tail = NULL;
    list->length = 0;

    return list;
}

node_t* list_insert( list_t* list, void* item )
{
    node_t* node = malloc(sizeof(node_t));
    node->value = item;
    node->next = NULL;
    node->prev = NULL;
    node->owner = NULL;
    list_append(list, node);

    return node;
}

void list_destroy( list_t* list )
{
    node_t* n = list->head;
    while( n )
    {
        free(n->value);
        n = n->next;
    }
}

void list_free( list_t* list )
{
    node_t* n = list->head;
    while( n )
    {
        node_t* s = n->next;
        free(n);
        n = s;
    }
}

void list_append( list_t* list, node_t* node )
{
    node->next = NULL;
    node->owner = list;
    if( !list->length )
    {
        list->head = node;
        list->tail = node;
        node->next = NULL;
        node->prev = NULL;
        list->length++;
    }else
    {
        list->tail->next = node;
        node->prev = list->tail;
        list->tail = node;
        list->length++;
    }
}

void list_append_after( list_t* list, node_t* before, node_t* node )
{
    node->owner = list;
    if( !list->length )
    {
        list_append(list, node);
    }else
    {
        if( before == NULL )
        {
            node->next = list->head;
            node->prev = NULL;
            list->head->prev = node;
            list->head = node;
            list->length++;
        }else 
        {
            if( before == list->tail )
            {
                list->tail = node;
            }else
            {
                before->next->prev = node;
                node->next = before->next;
            }
            node->prev = before;
            before->next = node;
            list->length++;
        }
    }
}

node_t* list_insert_after( list_t* list, node_t* before, void* item )
{
    node_t* node = malloc(sizeof(node_t));
    node->value = item;
    node->next = NULL;
    node->prev = NULL;
    node->owner = NULL;
    list_append_after(list, before, node);

    return node;
}

void list_append_before( list_t* list, node_t* after, node_t* node )
{
    node->owner = list;
    if( !list->length )
    {
        list_append(list, node);
    }else
    {
        if( after == NULL )
        {
            node->next = NULL;
            node->prev = list->tail;
            list->tail->next = node;
            list->tail = node;
            list->length++;
        }else if( after == list->head )
        {
            list->head = node;
        }else
        {
            after->prev->next = node;
            node->prev = after->prev;
        }
        node->next = after;
        after->prev = node;
        list->length++;
    }
}

node_t* list_insert_before( list_t* list, node_t* after, void* item )
{
    node_t* node = malloc(sizeof(node_t));
    node->value = item;
    node->next = NULL;
    node->prev = NULL;
    node->owner = NULL;
    list_append_before(list, after, node);

    return node;
}

node_t* list_find( list_t* list, void* value )
{
    foreach(item, list)
    {
        if( item->value == value )
        {
            return item;
        }
    }

    return NULL;
}

size_t list_index_of( list_t* list, void* value )
{
    size_t i = 0;
    foreach(item, list)
    {
        if( item->value == value )
        {
            return i;
        }
        i++;
    }

    return -1;
}

void list_remove( list_t* list, size_t index )
{
    if( index > list->length ) return;
    size_t i = 0;
    node_t* n = list->head;
    while( i < index )
    {
        n = n->next;
        i++;
    }
    list_delete(list, n);
}

void list_delete( list_t* list, node_t* node )
{
    if( node == list->head )
    {
        list->head = node->next;
    }
    if( node == list->tail )
    {
        list->tail = node->prev;
    }
    if( node->next )
    {
        node->next->prev = node->prev;
    }
    if( node->prev )
    {
        node->prev->next = node->next;
    }

    node->next = NULL;
    node->prev = NULL;
    node->owner = NULL;
    list->length--;
}

node_t* list_pop( list_t* list )
{
    if( !list->tail ) return NULL;
    node_t* node = list->tail;
    list_delete(list, node);

    return node;
}

node_t* list_dequeue( list_t* list )
{
    if( !list->head ) return NULL;
    node_t* node = list->head;
    list_delete(list, node);

    return node;
}

list_t* list_copy( list_t* list )
{
    list_t* copy = list_create();
    node_t* node = list->head;
    while( node )
    {
        list_insert(copy, node->value);
    }

    return copy;
}

void list_merge( list_t* target, list_t* source )
{
    foreach(node, source)
    {
        node->owner = target;
    }

    if( source->head )
    {
        source->head->prev = target->tail;
    }

    if( target->tail )
    {
        target->tail->next = source->head;
    }else
    {
        target->head = source->head;
    }

    if( source->tail )
    {
        target->tail = source->tail;
    }
    target->length += source->length;
    free(source);
}
