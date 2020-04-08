#include <string.h>

#include <kernel/idt.h>

#define ENTRY(X)(idt.entries[(X)])

typedef struct
{
    uint16_t base_low;
    uint16_t sel;
    uint8_t zero;
    uint8_t flags;
    uint16_t base_high;
} __attribute__((packed)) idt_entry_t;

typedef struct
{
    uint16_t limit;
    uintptr_t base;
} __attribute__((packed)) idt_pointer_t;

static struct
{
    idt_entry_t entries[256];
    idt_pointer_t pointer;
} idt __attribute__((used));

typedef void (*idt_gate_t)(void);

// Utility function to setup IDT entry
void idt_set_gate( uint8_t num, idt_gate_t base, uint16_t sel, uint8_t flags )
{
    ENTRY(num).base_low = ((uintptr_t)base & 0xFFFF);
    ENTRY(num).base_high = (uint16_t)((uintptr_t)base >> 16 ) & 0xFFFF;
    ENTRY(num).sel = sel;
    ENTRY(num).zero = 0;
    ENTRY(num).flags = flags | 0x60;
}

// Setup and then load IDT
void idt_initialize( void )
{
    // Setup IDT pointer and struct
    idt_pointer_t* idtptr = &idt.pointer;
    idtptr->limit = sizeof(idt.entries) - 1;
    idtptr->base = (uintptr_t)&ENTRY(0);
    memset(&ENTRY(0), 0, sizeof(idt.entries));

    idt_load((uintptr_t)idtptr); // Load IDT (idt.S)
}
