#include <tree.h>
#include <stdlib.h>
#include <stddef.h>

tree_t* tree_create( void )
{
    tree_t* tree = malloc(sizeof(tree_t));
    tree->nodes = 0;
    tree->root = NULL;

    return tree;
}

void tree_set_root( tree_t* tree, void* value )
{
    tree_node_t* root = tree_node_create(value);
    tree->root = root;
    tree->nodes = 1;
}

void tree_node_destroy( tree_node_t* node )
{
    foreach(child, node->children)
    {
        tree_node_destroy((tree_node_t*)child->value);
    }

    free(node->value);
}

void tree_destroy( tree_t* tree )
{
    if( tree->root )
    {
        tree_node_destroy(tree->root);
    }
}

static void tree_node_free( tree_node_t* node )
{
    if( !node ) return;
    foreach(child, node->children)
    {
        tree_node_free(child->value);
    }
    free(node);
}

void tree_free( tree_t* tree )
{
    tree_node_free(tree->root);
}

tree_node_t* tree_node_create( void* value )
{
    tree_node_t* node = malloc(sizeof(tree_node_t));
    node->value = value;
    node->children = list_create();
    node->parent = NULL;

    return node;
}

void tree_node_insert_child_node( tree_t* tree, tree_node_t* parent, tree_node_t* node )
{
    list_insert( parent->children, node );
    node->parent = parent;
    tree->nodes++;
}

tree_node_t* tree_node_insert_child( tree_t* tree, tree_node_t* parent, void* value )
{
    tree_node_t* node = tree_node_create(value);
    tree_node_insert_child_node(tree, parent, node);

    return node;
}

tree_node_t* tree_node_find_parent( tree_node_t* haystack, tree_node_t* needle )
{
    tree_node_t* found = NULL;
    foreach(child, haystack->children)
    {
        if( child->value == needle )
        {
            return haystack;
        }
        found = tree_node_find_parent((tree_node_t*)child->value, needle);
        if( found )
        {
            break;
        }
    }

    return found;
}

tree_node_t* tree_find_parent( tree_t* tree, tree_node_t* node )
{
    if( !tree->root ) return NULL;
    return tree_node_find_parent(tree->root, node);
}

size_t tree_count_children( tree_node_t* node )
{
    if( !node ) return 0;
    if( !node->children ) return 0;
    size_t count = node->children->length;
    foreach(child, node->children)
    {
        count += tree_count_children((tree_node_t*)child->value);
    }

    return count;
}

void tree_node_parent_remove( tree_t* tree, tree_node_t* parent, tree_node_t* node )
{
    tree->nodes -= tree_count_children(node) + 1;
    list_delete(parent->children, list_find(parent->children, node));
    tree_node_free(node);
}

void tree_node_remove( tree_t* tree, tree_node_t* node )
{
    tree_node_t* parent = node->parent;
    if( !parent )
    {
        if( node == tree->root )
        {
            tree->nodes = 0;
            tree->root = NULL;
            tree_node_free(node);
        }
    }
    tree_node_parent_remove(tree, parent, node);
}

void tree_remove( tree_t* tree, tree_node_t* node )
{
    tree_node_t* parent = node->parent;
    if( !parent ) return;
    tree->nodes--;
    list_delete(parent->children, list_find(parent->children, node));
    foreach(child, node->children)
    {
        ((tree_node_t*)child->value)->parent = parent;
    }

    list_merge(parent->children, node->children);
    free(node);
}

void tree_remove_reparent_root( tree_t* tree, tree_node_t* node )
{
    tree_node_t* parent = node->parent;
    if( !parent ) return;
    tree->nodes--;
    list_delete(parent->children, list_find(parent->children, node));
    foreach(child, node->children)
    {
        ((tree_node_t*)child->value)->parent = tree->root;
    }
    
    list_merge(tree->root->children, node->children);
    free(node);
}

void tree_break_off( tree_t* tree __attribute__((unused)), tree_node_t* node )
{
    tree_node_t* parent = node->parent;
    if( !parent ) return;
    list_delete(parent->children, list_find(parent->children, node));
}

tree_node_t* tree_node_find( tree_node_t* node, void* search, tree_comparator_t comparator )
{
    if( comparator(node->value, search) )
    {
        return node;
    }
    tree_node_t* found;
    foreach(child, node->children)
    {
        found = tree_node_find((tree_node_t*)child->value, search, comparator);
        if( found ) return found;
    }
    
    return NULL;
}

tree_node_t* tree_find( tree_t* tree, void* value, tree_comparator_t comparator )
{
    return tree_node_find( tree->root, value, comparator );
}
