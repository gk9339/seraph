#ifndef _KERNEL_SHM_H
#define _KERNEL_SHM_H

#define SHM_START 0xB0000000
#define SHM_PATH_SEPERATOR "."

struct shm_node;

typedef struct
{
    struct shm_node* parent;
    volatile uint8_t lock;
    uint32_t ref_count;

    uint32_t num_frames;
    uintptr_t* frames;
} shm_chunk_t;

typedef struct shm_node
{
    char name[256];
    shm_chunk_t* chunk;
} shm_node_t;

typedef struct
{
    shm_chunk_t* chunk;
    uint8_t volatile lock;
    
    uint32_t num_vaddrs;
    uintptr_t *vaddrs;
} shm_mapping_t;

void* shm_obtain( char * path, size_t * size );
int shm_release( char * path );

void shm_install( void );
void shm_release_all( process_t * proc );

#endif
