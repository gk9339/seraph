#ifndef _KERNEL_FS_H
#define _KERNEL_FS_H

#include <sys/stat.h>
#include <stdint.h> // intN_t
#include <stddef.h> // size_t

#define PATH_SEPARATOR '/'
#define PATH_SEPARATOR_STRING "/"
#define PATH_UP ".."
#define PATH_DOT "."

#define O_RDONLY     0x0000
#define O_WRONLY     0x0001
#define O_RDWR       0x0002
#define O_APPEND     0x0008
#define O_CREAT      0x0200
#define O_TRUNC      0x0400
#define O_EXCL       0x0800
#define O_NOFOLLOW   0x1000
#define O_PATH       0x2000
#define O_NONBLOCK   0x4000
#define O_DIRECTORY  0x8000

#define FS_FILE        0x01
#define FS_DIRECTORY   0x02
#define FS_CHARDEVICE  0x04
#define FS_BLOCKDEVICE 0x08
#define FS_PIPE        0x10
#define FS_SYMLINK     0x20
#define FS_MOUNTPOINT  0x40

struct fs_node; // Defined below

typedef uint32_t (*read_type_t)( struct fs_node*, uint32_t, uint32_t, uint8_t* ); // Read file
typedef uint32_t (*write_type_t)( struct fs_node*, uint32_t, uint32_t, uint8_t* ); // Write file
typedef void (*open_type_t)( struct fs_node*, unsigned int flags ); // Open fs_node
typedef void (*close_type_t)( struct fs_node* ); // Close fs_node
typedef struct dirent* (*readdir_type_t)( struct fs_node*, uint32_t ); 
typedef struct fs_node* (*finddir_type_t)( struct fs_node*, char *name );
typedef int (*create_type_t)( struct fs_node*, char *name, uint16_t permission );
typedef int (*unlink_type_t)( struct fs_node*, char *name );
typedef int (*mkdir_type_t)( struct fs_node*, char *name, uint16_t permission );
typedef int (*ioctl_type_t)( struct fs_node*, int request, void * argp );
typedef int (*get_size_type_t)( struct fs_node* );
typedef int (*chmod_type_t)( struct fs_node*, int mode );
typedef int (*symlink_type_t)( struct fs_node*, char * name, char * value );
typedef int (*readlink_type_t)( struct fs_node*, char * buf, size_t size );
typedef int (*selectcheck_type_t)( struct fs_node* );
typedef int (*selectwait_type_t)( struct fs_node*, void* process );
typedef int (*chown_type_t)( struct fs_node*, int, int );
typedef void (*truncate_type_t)( struct fs_node* );

typedef struct fs_node 
{
    char name[256];  // The filename
    void* device;    // Device object (optional)
    uint32_t mask;   // The permissions mask
    uint32_t uid;    // The owning user
    uint32_t gid;    // The owning group
    uint32_t type;   // Type of tile
    uint32_t inode;  // Inode number
    uint32_t length; // Size of the file, in byte
    uint32_t impl;   // Used to keep track which fs it belongs to
                              
    // Times
    uint32_t atime; // Accessed
    uint32_t mtime; // Modified
    uint32_t ctime; // Created

    // File operations
    read_type_t read;
    write_type_t write;
    open_type_t open;
    close_type_t close;
    readdir_type_t readdir;
    finddir_type_t finddir;
    create_type_t create;
    mkdir_type_t mkdir;
    ioctl_type_t ioctl;
    get_size_type_t get_size;
    chmod_type_t chmod;
    unlink_type_t unlink;
    symlink_type_t symlink;
    readlink_type_t readlink;
    truncate_type_t truncate;

    struct fs_node *ptr; // Alias pointer, for symlinks
    int32_t refcount;
    uint32_t nlink;

    selectcheck_type_t selectcheck;
    selectwait_type_t selectwait;

    chown_type_t chown;
} fs_node_t;

struct dirent
{
    uint32_t ino; // Inode
    char name[256]; // Directory name
};

struct vfs_entry 
{
    char* name; // Name of vfs entry
    fs_node_t* file; // Pointer to file struct
    char* device;
    char* fs_type;
};

extern fs_node_t* fs_root;

int has_permission( fs_node_t *node, int permission_bit ); // Does current process user have permission
uint32_t read_fs( fs_node_t* node, uint32_t offset, uint32_t size, uint8_t* buffer );
uint32_t write_fs( fs_node_t* node, uint32_t offset, uint32_t size, uint8_t* buffer );
void open_fs( fs_node_t* node, unsigned int flags );
void close_fs( fs_node_t* node );
struct dirent* readdir_fs( fs_node_t* node, uint32_t index );
fs_node_t* finddir_fs( fs_node_t* node, char *name );
int mkdir_fs( char* name, uint16_t permission );
int create_file_fs( char* name, uint16_t permission );
fs_node_t* kopen( char* filename, uint32_t flags );
char *canonicalize_path( char* cwd, char* input );
fs_node_t* clone_fs( fs_node_t* source );
int ioctl_fs( fs_node_t* node, int request, void* argp );
int chmod_fs( fs_node_t* node, int mode );
int chown_fs( fs_node_t* node, int uid, int gid );
int unlink_fs( char* name );
int symlink_fs( char* value, char* name );
int readlink_fs( fs_node_t* node, char* buf, size_t size );
int selectcheck_fs( fs_node_t* node );
int selectwait_fs( fs_node_t* node, void* process );
void truncate_fs( fs_node_t* node );

void vfs_initialize( void );
void* vfs_mount( char* path, fs_node_t* local_root );
typedef fs_node_t* ( *vfs_mount_callback)(char* arg, char* mount_point );
int vfs_register( char* name, vfs_mount_callback callback );
int vfs_mount_type( char* type, char* arg, char* mountpoint );
void vfs_lock( fs_node_t* node );
void map_vfs_directory( char* );

void debug_print_vfs_tree( char** );

void zero_initialize( void );
void null_initialize( void );

#endif
