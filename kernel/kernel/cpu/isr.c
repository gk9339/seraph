#include <stdio.h>

#include <kernel/isr.h>
#include <kernel/idt.h>
#include <kernel/irq.h>
#include <kernel/kernel.h>
#include <kernel/symbols.h>

#define ISR_COUNT 32

static struct
{
    uint8_t index;
    void (*stub)(void);
} isrs[32+1] __attribute__((used));

static irq_handler_t isr_routines[256] = {0};

// Adds routine to ISR table
void isr_install_handler( size_t isr, irq_handler_t handler )
{
    isr_routines[isr] = handler;
}

// Sets location in ISR table to NULL
void isr_uninstall_handler( size_t isr )
{
    isr_routines[isr] = NULL;
}

// Setup ISR function table, setup syscall vector as additional ISR, install IDT gates
void isr_initialize( void )
{
    char buffer[16];
    for( uint8_t i = 0; i < ISR_COUNT; i++ )
    {
        sprintf(buffer, "_isr%d", i);
        isrs[i].index = i;
        isrs[i].stub = symbol_find(buffer);
    }
    isrs[ISR_COUNT].index = SYSCALL_VECTOR;
    isrs[ISR_COUNT].stub = symbol_find("_isr127");

    for( uint8_t i = 0; i < ISR_COUNT + 1; i++ )
    {
        idt_set_gate(isrs[i].index, isrs[i].stub, 0x08, 0x8E);
    }
}

static const char* exception_messages[32] =
{
    "Division by zero",
    "Debug",
    "Non-maskable interrupt",
    "Breakpoint",
    "Detected overflow",
    "Out-of-bounds",
    "Invalid opcode",
    "No coprocessor",
    "Double fault",
    "Coprocessor segment overrun",
    "Bad TSS",
    "Segment not present",
    "Stack fault",
    "General protection fault",
    "Page fault",
    "Unknown interrupt",
    "Coprocessor fault",
    "Alignment check",
    "Machine check",
    "Reserved",
    "Reserved",
    "Reserved",
    "Reserved",
    "Reserved",
    "Reserved",
    "Reserved",
    "Reserved",
    "Reserved",
    "Reserved",
    "Reserved",
    "Reserved",
    "Reserved"
};

// Handle ISR, if handler exists, kernel panic otherwise
void fault_handler( struct regs* r )
{
    irq_handler_t handler = isr_routines[r->int_no];
    if(handler)
    {
        handler(r);
    }else
    {
        printf("\n%s\n", exception_messages[r->int_no]);
        KPANIC("Process caused an unhandled exception", r);
        STOP;
    }
}
