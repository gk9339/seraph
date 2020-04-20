#ifndef _TREE_H
#define _TREE_H

#ifdef __cplusplus
extern "C" {
#endif

#include <list.h>
#include <stdint.h>

typedef struct tree_node
{
    void* value;
    list_t* children;
    struct tree_node* parent;
} tree_node_t;

typedef struct
{
    size_t nodes;
    tree_node_t* root;
} tree_t;

typedef uint8_t(*tree_comparator_t)(void*, void*);

tree_t* tree_create( void );
void tree_set_root( tree_t* tree, void* value );
void tree_node_destroy( tree_node_t* node );
void tree_destroy( tree_t* tree );
void tree_free( tree_t* tree );
tree_node_t* tree_node_create( void* value );
void tree_node_insert_child_node( tree_t* tree, tree_node_t* parent, tree_node_t* node );
tree_node_t* tree_node_insert_child(tree_t* tree, tree_node_t* parent, void* value);
tree_node_t* tree_find_parent( tree_t* tree, tree_node_t* node );
tree_node_t* tree_node_find_parent(tree_node_t* haystack, tree_node_t* needle);
void tree_node_parent_remove(tree_t* tree, tree_node_t* parent, tree_node_t* node);
void tree_node_remove(tree_t* tree, tree_node_t * node);
void tree_remove(tree_t* tree, tree_node_t* node);
tree_node_t* tree_find(tree_t* tree, void* value, tree_comparator_t comparator);
tree_node_t* tree_node_find( tree_node_t* node, void* search, tree_comparator_t comparator );
void tree_break_off(tree_t* tree, tree_node_t* node);
void tree_remove_reparent_root( tree_t* tree, tree_node_t* node );
size_t tree_count_children( tree_node_t* node );

#ifdef __cplusplus
}
#endif

#endif
