#include <kernel/shm.h>
#include <kernel/mem.h>
#include <kernel/spinlock.h>
#include <kernel/process.h>
#include <kernel/kconfig.h>
#include <stddef.h>
#include <sys/types.h>
#include <tree.h>
#include <list.h>
#include <stdlib.h>
#include <string.h>

static spin_lock_t shm_lock;
tree_t* shm_tree = NULL;

void shm_initialize( void )
{
    shm_tree = tree_create();
    tree_set_root(shm_tree, NULL);
}

static shm_node_t* get_node_recursive( char* shm_path, int create, tree_node_t* from )
{
    char *pch, *save;
    pch = strtok_r(shm_path, SHM_PATH_SEPARATOR, &save);

    tree_node_t* tnode = from;
    foreach(node, tnode->children)
    {
        tree_node_t* _node = (tree_node_t*)node->value;
        shm_node_t* shm_node = (shm_node_t*)_node->value;

        if( !strcmp(shm_node->name, pch) )
        {
            if( *save == '\0' )
            {
                return shm_node;
            }
            return get_node_recursive(save, create, _node);
        }
    }

    /* The next node in sequence was not found */
    if( create )
    {
        shm_node_t* new_shm_node = malloc(sizeof(shm_node_t));
        memcpy(new_shm_node->name, pch, strlen(pch) + 1);
        new_shm_node->chunk = NULL;

        tree_node_t* nnode = tree_node_insert_child(shm_tree, from, new_shm_node);

        if( *save == '\0' )
        {
            return new_shm_node;
        }

        return get_node_recursive(save, create, nnode);
    }else
    {
        return NULL;
    }
}

static shm_node_t* get_node( char* shm_path, int create )
{
    char* _path = malloc(strlen(shm_path) + 1);
    memcpy(_path, shm_path, strlen(shm_path) + 1);

    shm_node_t* node = get_node_recursive(_path, create, shm_tree->root);

    free(_path);
    return node;
}

static shm_chunk_t* create_chunk( shm_node_t* parent, size_t size )
{
    if( !size ) return NULL;

    shm_chunk_t* chunk = malloc(sizeof(shm_chunk_t));
    if( chunk == NULL )
    {
        return NULL;
    }

    chunk->parent = parent;
    chunk->lock = 0;
    chunk->ref_count = 1;

    chunk->num_frames = (size / 0x1000) + ((size & 0x1000)? 1:0);
    chunk->frames = malloc(chunk->num_frames * sizeof(uintptr_t));
    if( chunk->frames == NULL )
    {
        free(chunk);
        return NULL;
    }

    for( uint32_t i = 0; i < chunk->num_frames; i++ )
    {
        page_t tmp = { 0 };
        alloc_frame(&tmp, 0, 0);
        chunk->frames[i] = tmp.frame;
    }

    return chunk;
}

static int release_chunk( shm_chunk_t* chunk )
{
    if( chunk )
    {
        chunk->ref_count--;

        if( chunk->ref_count < 1 )
        {
            for( uint32_t i = 0; i < chunk->num_frames; i++ )
            {
                clear_frame(chunk->frames[i] * 0x1000);
            }

            chunk->parent->chunk = NULL;
            free(chunk->frames);
            free(chunk);
        }

        return 0;
    }

    return -1;
}

static uintptr_t proc_sbrk( uintptr_t num_pages, process_t* proc )
{
    uintptr_t initial = proc->image.shm_heap;

    if( initial % 0x1000 )
    {
        initial += 0x1000 - (initial % 0x1000);
        proc->image.shm_heap = initial;
    }
    proc->image.shm_heap += num_pages * 0x1000;

    return initial;
}

static void* map_in( shm_chunk_t* chunk, process_t* proc )
{
    if( !chunk ) return NULL;

    shm_mapping_t* mapping = malloc(sizeof(shm_mapping_t));
    mapping->chunk = chunk;
    mapping->num_vaddrs = chunk->num_frames;
    mapping->vaddrs = malloc(mapping->num_vaddrs * sizeof(uintptr_t));

    uintptr_t last_address = SHM_START;
    foreach(node, proc->shm_mappings)
    {
        shm_mapping_t* m = node->value;
        if( m->vaddrs[0] > last_address )
        {
            size_t gap = (uintptr_t)m->vaddrs[0] - last_address;
            if( gap >= mapping->num_vaddrs * 0x1000 )
            {
                for( unsigned int i = 0; i < chunk->num_frames; i++ )
                {
                    page_t* page = get_page(last_address + i * 0x1000, 1, proc->thread.page_directory);
                    page->frame = chunk->frames[i] & 0xfffff;
                    alloc_frame(page, 0, 1);
                    invalidate_tables_at(last_address + i * 0x1000);
                    mapping->vaddrs[i] = last_address + i * 0x1000;
                }

                list_insert_before(proc->shm_mappings, node, mapping);

                return (void*)mapping->vaddrs[0];
            }
        }
        last_address = m->vaddrs[0] + m->num_vaddrs * 0x1000;
    }

    if( proc->image.shm_heap > last_address )
    {
        size_t gap = proc->image.shm_heap - last_address;
        if( gap >= mapping->num_vaddrs * 0x1000 )
        {
            for( unsigned int i = 0; i < chunk->num_frames; i++ )
            {
                page_t * page = get_page(last_address + i * 0x1000, 1, proc->thread.page_directory);
                page->frame = chunk->frames[i] & 0xfffff;
                alloc_frame(page, 0, 1);
                invalidate_tables_at(last_address + i * 0x1000);
                mapping->vaddrs[i] = last_address + i * 0x1000;
            }

            list_insert(proc->shm_mappings, mapping);

            return (void*)mapping->vaddrs[0];
        }
    }

    for( uint32_t i = 0; i < chunk->num_frames; i++ )
    {
        uintptr_t new_vpage = proc_sbrk(1, proc);

        page_t* page = get_page(new_vpage, 1, proc->thread.page_directory);

        page->frame = chunk->frames[i] & 0xfffff;
        alloc_frame(page, 0, 1);
        invalidate_tables_at(new_vpage);
        mapping->vaddrs[i] = new_vpage;
    }

    list_insert(proc->shm_mappings, mapping);

    return (void*)mapping->vaddrs[0];
}

static size_t chunk_size( shm_chunk_t* chunk )
{
    return (size_t)(chunk->num_frames * 0x1000);
}

void* shm_obtain( char* path, size_t* size )
{
    spin_lock(shm_lock);
    process_t* proc = (process_t*)current_process;

    if( proc->group != 0 )
    {
        proc = process_from_pid(proc->group);
    }

    shm_node_t* node = get_node(path, 1);
    shm_chunk_t* chunk = node->chunk;

    if( chunk == NULL )
    {
        if( size == 0 )
        {
            spin_unlock(shm_lock);
            return NULL;
        }

        chunk = create_chunk(node, *size);
        if( chunk == NULL )
        {
            spin_unlock(shm_lock);
            return NULL;
        }

        node->chunk = chunk;
    }else
    {
        chunk->ref_count++;
    }

    void* vshm_start = map_in(chunk, proc);
    *size = chunk_size(chunk);

    spin_unlock(shm_lock);
    invalidate_page_tables();

    return vshm_start;
}

int shm_release( char* path )
{
    spin_lock(shm_lock);
    process_t* proc = (process_t*)current_process;

    if( proc->group != 0 )
    {
        proc = process_from_pid(proc->group);
    }

    shm_node_t* _node = get_node(path, 0);
    if( !_node )
    {
        spin_unlock(shm_lock);
        return 1;
    }
    shm_chunk_t* chunk = _node->chunk;

    node_t* node = NULL;
    foreach(n, proc->shm_mappings)
    {
        shm_mapping_t* m = (shm_mapping_t*)n->value;
        if( m->chunk == chunk )
        {
            node = n;
            break;
        }
    }
    if( node == NULL )
    {
        spin_unlock(shm_lock);
        return 1;
    }

    shm_mapping_t* mapping = (shm_mapping_t*)node->value;

    for( uint32_t i = 0; i < mapping->num_vaddrs; i++ )
    {
        page_t* page = get_page(mapping->vaddrs[i], 0, proc->thread.page_directory);
        memset(page, 0, sizeof(page_t));
    }
    invalidate_page_tables();

    release_chunk(chunk);
    list_delete(proc->shm_mappings, node);
    free(node);
    free(mapping);

    spin_unlock(shm_lock);
    return 0;
}

void shm_release_all( process_t* proc )
{
    spin_lock(shm_lock);

    node_t* node;
    while( (node = list_pop(proc->shm_mappings)) != NULL )
    {
        shm_mapping_t* mapping = node->value;
        release_chunk(mapping->chunk);
        free(mapping);
        free(node);
    }

    list_free(proc->shm_mappings);
    proc->shm_mappings->head = proc->shm_mappings->tail = NULL;
    proc->shm_mappings->length = 0;

    spin_unlock(shm_lock);
}
