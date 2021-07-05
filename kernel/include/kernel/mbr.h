#ifndef _KERNEL_MBR_H
#define _KERNEL_MBR_H

typedef struct
{
    uint8_t bootstrap[446];
    partition_t partitions[4];
    uint8_t signature[2];
} __attribute__((packed)) mbr_t;

void mbr_initialize( void );

#endif
