#include <kernel/ustar.h>
#include <kernel/fs.h>
#include <kernel/kernel.h>
#include <string.h>
#include <stdlib.h>

struct ustar_dev 
{
    fs_node_t* device;
    unsigned int length;
};

struct ustar 
{
    char filename[100];
    char mode[8];
    char ownerid[8];
    char groupid[8];

    char size[12];
    char mtime[12];

    char checksum[8];
    char type[1];
    char link[100];

    char ustar[6];
    char version[2];

    char owner[32];
    char group[32];

    char dev_major[8];
    char dev_minor[8];

    char prefix[155];
};

static int ustar_from_offset( struct ustar_dev* self, unsigned int offset, struct ustar* out );
static fs_node_t* file_from_ustar( struct ustar_dev* self, struct ustar* file, unsigned int offset );

static unsigned int interpret_uid( struct ustar* file )
{
    return ((file->ownerid[0] - '0') << 18) |
           ((file->ownerid[1] - '0') << 15) |
           ((file->ownerid[2] - '0') << 12) |
           ((file->ownerid[3] - '0') <<  9) |
           ((file->ownerid[4] - '0') <<  6) |
           ((file->ownerid[5] - '0') <<  3) |
           ((file->ownerid[6] - '0') <<  0);
}

static unsigned int interpret_gid( struct ustar* file )
{
    return ((file->groupid[0] - '0') << 18) |
           ((file->groupid[1] - '0') << 15) |
           ((file->groupid[2] - '0') << 12) |
           ((file->groupid[3] - '0') <<  9) |
           ((file->groupid[4] - '0') <<  6) |
           ((file->groupid[5] - '0') <<  3) |
           ((file->groupid[6] - '0') <<  0);
}

static unsigned int interpret_mode( struct ustar* file )
{
    return ((file->mode[0] - '0') << 18) |
           ((file->mode[1] - '0') << 15) |
           ((file->mode[2] - '0') << 12) |
           ((file->mode[3] - '0') <<  9) |
           ((file->mode[4] - '0') <<  6) |
           ((file->mode[5] - '0') <<  3) |
           ((file->mode[6] - '0') <<  0);
}

static unsigned int interpret_size( struct ustar* file )
{
    return ((file->size[ 0] - '0') << 30) |
           ((file->size[ 1] - '0') << 27) |
           ((file->size[ 2] - '0') << 24) |
           ((file->size[ 3] - '0') << 21) |
           ((file->size[ 4] - '0') << 18) |
           ((file->size[ 5] - '0') << 15) |
           ((file->size[ 6] - '0') << 12) |
           ((file->size[ 7] - '0') <<  9) |
           ((file->size[ 8] - '0') <<  6) |
           ((file->size[ 9] - '0') <<  3) |
           ((file->size[10] - '0') <<  0);
}

static unsigned int round_to_512( unsigned int i )
{
    unsigned int t = i % 512;

    if (!t) return i;
    return i + (512 - t);
}

static int count_slashes( char* string )
{
    int i = 0;
    char* s = strstr(string, "/");
    while( s )
    {
        if( *(s+1) == '\0' ) return i;
        i++;
        s = strstr(s+1,"/");
    }
    return i;
}

static struct dirent * readdir_ustar_root( fs_node_t*node, uint32_t index )
{
    if( index == 0 )
    {
        struct dirent * out = malloc(sizeof(struct dirent));
        memset(out, 0x00, sizeof(struct dirent));
        out->ino = 0;
        strcpy(out->name, ".");
        return out;
    }

    if( index == 1 )
    {
        struct dirent * out = malloc(sizeof(struct dirent));
        memset(out, 0x00, sizeof(struct dirent));
        out->ino = 0;
        strcpy(out->name, "..");
        return out;
    }

    index -= 2;

    struct ustar_dev* self = node->device;
    /* Go through each file and pick the ones are at the root */
    /* Root files will have no /, so this is easy */
    unsigned int offset = 0;
    struct ustar* file = malloc(sizeof(struct ustar));
    while( offset < self->length )
    {
        int status = ustar_from_offset(self, offset, file);

        if( !status )
        {
            free(file);
            return NULL;
        }

        char filename_workspace[256];

        memset(filename_workspace, 0, 256);
        strncat(filename_workspace, file->prefix, 155);
        strncat(filename_workspace, file->filename, 100);

        if( !count_slashes(filename_workspace) )
        {
            char* slash = strstr(filename_workspace,"/");
            if( slash ) *slash = '\0'; /* remove trailing slash */
            if( strlen(filename_workspace) )
            {
                if( index == 0 )
                {
                    struct dirent * out = malloc(sizeof(struct dirent));
                    memset(out, 0x00, sizeof(struct dirent));
                    out->ino = offset;
                    strcpy(out->name, filename_workspace);
                    free(file);
                    return out;
                }else
                {
                    index--;
                }
            }
        }

        offset += 512;
        offset += round_to_512(interpret_size(file));
    }

    free(file);
    return NULL;
}

static uint32_t read_ustar( fs_node_t* node, uint32_t offset, uint32_t size, uint8_t * buffer )
{
    struct ustar_dev* self = node->device;
    struct ustar* file = malloc(sizeof(struct ustar));
    ustar_from_offset(self, node->inode, file);
    size_t file_size = interpret_size(file);

    if( offset > file_size ) return 0;
    if( offset + size > file_size )
    {
        size = file_size - offset;
    }

    free(file);

    return read_fs(self->device, offset + node->inode + 512, size, buffer);
}

static struct dirent * readdir_ustar( fs_node_t*node, uint32_t index )
{
    if( index == 0 )
    {
        struct dirent * out = malloc(sizeof(struct dirent));
        memset(out, 0x00, sizeof(struct dirent));
        out->ino = 0;
        strcpy(out->name, ".");
        return out;
    }

    if( index == 1 )
    {
        struct dirent * out = malloc(sizeof(struct dirent));
        memset(out, 0x00, sizeof(struct dirent));
        out->ino = 0;
        strcpy(out->name, "..");
        return out;
    }

    index -= 2;

    struct ustar_dev* self = node->device;

    /* Go through each file and pick the ones are at the root */
    /* Root files will have no /, so this is easy */
    unsigned int offset = node->inode;

    /* Read myself */
    struct ustar* file = malloc(sizeof(struct ustar));
    int status = ustar_from_offset(self, node->inode, file);
    char my_filename[256];

    /* Figure out my own filename, with forward slash */
    memset(my_filename, 0, 256);
    strncat(my_filename, file->prefix, 155);
    strncat(my_filename, file->filename, 100);

    while( offset < self->length )
    {
        ustar_from_offset(self, offset, file);

        if( !status )
        {
            free(file);
            return NULL;
        }

        char filename_workspace[256];
        memset(filename_workspace, 0, 256);
        strncat(filename_workspace, file->prefix, 155);
        strncat(filename_workspace, file->filename, 100);

        if( strstr(filename_workspace, my_filename) == filename_workspace )
        {
            if( !count_slashes(filename_workspace + strlen(my_filename)) )
            {
                if( strlen(filename_workspace + strlen(my_filename)) )
                {
                    if( index == 0 )
                    {
                        char* slash = strstr(filename_workspace+strlen(my_filename),"/");
                        if( slash ) *slash = '\0'; /* remove trailing slash */
                        struct dirent * out = malloc(sizeof(struct dirent));
                        memset(out, 0x00, sizeof(struct dirent));
                        out->ino = offset;
                        strcpy(out->name, filename_workspace+strlen(my_filename));
                        free(file);
                        return out;
                    }else
                    {
                        index--;
                    }
                }
            }
        }

        offset += 512;
        offset += round_to_512(interpret_size(file));
    }

    free(file);
    return NULL;
}

static fs_node_t* finddir_ustar( fs_node_t*node, char*name )
{
    struct ustar_dev* self = node->device;

    /* find my own filename */
    struct ustar* file = malloc(sizeof(struct ustar));
    ustar_from_offset(self, node->inode, file);

    char my_filename[256];
    /* Figure out my own filename, with forward slash */
    memset(my_filename, 0, 256);
    strncat(my_filename, file->prefix, 155);
    strncat(my_filename, file->filename, 100);

    /* Append name */
    strncat(my_filename, name, strlen(name));

    unsigned int offset = node->inode;
    while( offset < self->length )
    {
        int status = ustar_from_offset(self, offset, file);

        if( !status )
        {
            free(file);
            return NULL;
        }

        char filename_workspace[256];
        memset(filename_workspace, 0, 256);
        strncat(filename_workspace, file->prefix, 155);
        strncat(filename_workspace, file->filename, 100);

        if( filename_workspace[strlen(filename_workspace)-1] == '/' )
        {
            filename_workspace[strlen(filename_workspace)-1] = '\0';
        }
        if( !strcmp(filename_workspace, my_filename) )
        {
            return file_from_ustar(self, file, offset);
        }

        offset += 512;
        offset += round_to_512(interpret_size(file));
    }


    free(file);
    return NULL;
}

static int readlink_ustar( fs_node_t* node, char* buf, size_t size )
{
    struct ustar_dev* self = node->device;
    struct ustar* file = malloc(sizeof(struct ustar));
    ustar_from_offset(self, node->inode, file);

    if( size < strlen(file->link) + 1 )
    {
        memcpy(buf, file->link, size-1);
        buf[size-1] = '\0';
        free(file);
        return size-1;
    }else
    {
        memcpy(buf, file->link, strlen(file->link) + 1);
        free(file);
        return strlen(file->link);
    }
}

static fs_node_t* file_from_ustar( struct ustar_dev* self, struct ustar* file, unsigned int offset )
{
    fs_node_t* fs = malloc(sizeof(fs_node_t));
    memset(fs, 0, sizeof(fs_node_t));
    fs->device = self;
    fs->inode  = offset;
    fs->impl   = 0;
    char filename_workspace[256];
    memcpy(fs->name, filename_workspace, strlen(filename_workspace)+1);

    fs->uid = interpret_uid(file);
    fs->gid = interpret_gid(file);
    fs->length = interpret_size(file);
    fs->mask = interpret_mode(file);
    fs->nlink = 0; /* Unsupported */
    fs->type = FS_FILE;
    if( file->type[0] == '5' )
    {
        fs->type = FS_DIRECTORY;
        fs->readdir = readdir_ustar;
        fs->finddir = finddir_ustar;
    }else if( file->type[0] == '1' )
    {
        /* go through file and find target, reassign inode to point to that */
    }else if( file->type[0] == '2' )
    {
        fs->type = FS_SYMLINK;
        fs->readlink = readlink_ustar;
    }else
    {
        fs->type = FS_FILE;
        fs->read = read_ustar;
    }
    free(file);
    
    return fs;
}

static fs_node_t* finddir_ustar_root( fs_node_t*node, char*name )
{
    struct ustar_dev* self = node->device;

    unsigned int offset = 0;
    struct ustar* file = malloc(sizeof(struct ustar));
    while( offset < self->length )
    {
        int status = ustar_from_offset(self, offset, file);

        if( !status )
        {
            free(file);
            return NULL;
        }

        char filename_workspace[256];
        memset(filename_workspace, 0, 256);
        strncat(filename_workspace, file->prefix, 155);
        strncat(filename_workspace, file->filename, 100);

        if( count_slashes(filename_workspace) )
        {
            /* skip */
        }else
        {
            char* slash = strstr(filename_workspace,"/");
            if( slash ) *slash = '\0';
            if( !strcmp(filename_workspace, name) )
            {
                return file_from_ustar(self, file, offset);
            }
        }

        offset += 512;
        offset += round_to_512(interpret_size(file));
    }

    free(file);
    return NULL;
}

static int ustar_from_offset( struct ustar_dev* self, unsigned int offset, struct ustar* out )
{
    read_fs(self->device, offset, sizeof(struct ustar), (unsigned char*)out);
    if( out->ustar[0] != 'u' ||
        out->ustar[1] != 's' ||
        out->ustar[2] != 't' ||
        out->ustar[3] != 'a' ||
        out->ustar[4] != 'r' )
    {
        return 0;
    }
    return 1;
}

static int tokenize( char* str, char* sep, char** buf )
{
    char* pch_i;
    char* save_i;
    int argc = 0;

    pch_i = strtok_r(str, sep, &save_i);
    if( !pch_i ) return 0;

    while( pch_i != NULL )
    {
        buf[argc] = (char*)pch_i;
        argc++;
        pch_i = strtok_r(NULL, sep, &save_i);
    }
    buf[argc] = NULL;

    return argc;
}

static fs_node_t* ustar_mount( char* device, char* mount_path __attribute__((unused)) )
{
    char* arg = strdup(device);
    char* argv[10];
    tokenize(arg, ",", argv);

    fs_node_t* dev = kopen(argv[0], 0);
    free(arg);

    if( !dev )
    {
        return NULL;
    }

    struct ustar_dev* self = malloc(sizeof(struct ustar_dev));

    self->device = dev;
    self->length = dev->length;

    fs_node_t* root = malloc(sizeof(fs_node_t));
    memset(root, 0, sizeof(fs_node_t));

    root->uid = 0;
    root->gid = 0;
    root->length = 0;
    root->mask = 0555;
    root->readdir = readdir_ustar_root;
    root->finddir = finddir_ustar_root;
    root->type = FS_DIRECTORY;
    root->device = self;

    return root;
}

int ustar_initialize( void )
{
    vfs_register("ustar", ustar_mount);

    return 0;
}
