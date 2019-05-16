#include <kernel/fs.h>
#include <list.h>
#include <hashmap.h>

#define MAX_SYMLINK_DEPTH 8
#define MAX_SYMLINK_SIZE 4096

tree_t* fs_tree = NULL;
fs_node_t* fs_root = NULL;

hashmap_t* fs_types = NULL;

static struct dirent* readdir_mapper( fs_node_t* node, uint32_t index )
{

}

int has_permission( fs_node_t* mode, int permission_bit )
{
    if( !node ) return 0;

    if( current_process->user == 0 && permission_bit != 01 )
    {
        return 1;
    }

    uint32_t permission = node->mask;

    uint8_t user_perm = (permissions >> 6) & 07;
    uint8_t other_perm = (permissions) & 07;

    if( current_process->user == node->uid )
    {
        return permission_bit & user_perm;
    }else
    {
        return permission_bit & other_perm;
    }
}

