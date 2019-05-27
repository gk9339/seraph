#include <string.h>

#include <kernel/gdt.h>
#include <kernel/tss.h>

void gdt_flush( uintptr_t );
static void write_tss( int32_t num, uint16_t ss0, uint32_t esp0 );

#define ENTRY(X)(gdt.entries[(X)])

typedef struct
{
    uint16_t limit_low;     /* Limits */
    uint16_t base_low;      /* Segment address */
    uint8_t base_middle;
    uint8_t access;         /* Addess modes */
    uint8_t granularity;
    uint8_t base_high;
} __attribute__((packed)) gdt_entry_t;

typedef struct
{
    uint16_t limit;
    uintptr_t base;
} __attribute__((packed)) gdt_pointer_t;

static struct {
    gdt_entry_t entries[6];
    gdt_pointer_t pointer;
    tss_entry_t tss;
} gdt __attribute__((used));

void gdt_set_gate( uint8_t num, uint64_t base, uint64_t limit, uint8_t access, uint8_t gran )
{
    /* Base address */
    ENTRY(num).base_low = (base & 0xFFFF);
    ENTRY(num).base_middle = (base >> 16) & 0xFF;
    ENTRY(num).base_high = (base >> 24) & 0xFF;

    /* Limits */
    ENTRY(num).limit_low = (limit & 0xFFFF);
    ENTRY(num).granularity = (limit >> 16) & 0x0F;

    /* Granularity */
    ENTRY(num).granularity |= (gran & 0xF0);

    /* Access flags */
    ENTRY(num).access = access;
}

void gdt_initialize( void )
{
    gdt_pointer_t *gdtptr = &gdt.pointer;
    gdtptr->limit = sizeof(gdt.entries) - 1;
    gdtptr->base = (uintptr_t)&ENTRY(0);

    gdt_set_gate(0, 0, 0, 0, 0);                /* NULL segment */
    gdt_set_gate(1, 0, 0xFFFFFFFF, 0x9A, 0xCF); /* Code segment */
    gdt_set_gate(2, 0, 0xFFFFFFFF, 0x92, 0xCF); /* Data segment */
    gdt_set_gate(3, 0, 0xFFFFFFFF, 0xFA, 0xCF); /* User code */
    gdt_set_gate(4, 0, 0xFFFFFFFF, 0xF2, 0xCF); /* User data */

    write_tss(5, 0x10, 0x0);

    gdt_flush((uintptr_t)gdtptr);
    tss_flush();
}

static void write_tss( int32_t num, uint16_t ss0, uint32_t esp0 )
{
    tss_entry_t* tss = &gdt.tss;
    uintptr_t base = (uintptr_t)tss;
    uintptr_t limit = base + sizeof(*tss);

    /* Add the TSS descriptor to the GDT */
    gdt_set_gate(num, base, limit, 0xE9, 0x00);

    memset(tss, 0x0, sizeof(*tss));

    tss->ss0 = ss0;
    tss->esp0 = esp0;
    tss->cs = 0x0b;
    tss->ss = 0x13;
    tss->ds = 0x13;
    tss->es = 0x13;
    tss->fs = 0x13;
    tss->gs = 0x13;

    tss->iomap_base = sizeof(*tss);
}

void set_kernel_stack( uintptr_t stack )
{
    gdt.tss.esp0 = stack;
}
