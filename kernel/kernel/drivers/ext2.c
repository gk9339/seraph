#include <kernel/ext2.h>
#include <kernel/fs.h>
#include <kernel/kernel.h>

// Placeholder ext2 mount
static fs_node_t* ext2_fs_mount( char* device, char* mount_path )
{
    (void)device, (void)mount_path;
    KPANIC("ext2 mount not implemented", NULL);
    return NULL;
}

int ext2_initialize( void )
{
    vfs_register("ext2", ext2_fs_mount);

    return 0;
}
