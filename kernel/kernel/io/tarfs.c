#include <kernel/fs.h>
#include <kernel/kernel.h>

fs_node_t* tar_mount( char* device, char* mount_path )
{
    KPANIC("tar mount not implemented", NULL);
    return NULL;
}

int tarfs_initialize( void )
{
    vfs_register("tar", tar_mount);

    return 0;
}
