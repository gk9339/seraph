#include <stdio.h>

#include <kernel/isr.h>
#include <kernel/idt.h>
#include <kernel/irq.h>
#include <kernel/kernel.h>
#include <kernel/symbols.h>

#define ISR_COUNT 32

static struct
{
    size_t index;
    void (*stub)(void);
} isrs[32+1] __attribute__((used));

static irq_handler_t isr_routines[256] = {0};

void isr_install_handler( size_t isr, irq_handler_t handler )
{
    isr_routines[isr] = handler;
}

void isr_uninstall_handler( size_t isr )
{
    isr_routines[isr] = 0;
}

void isr_initialize( void )
{
    char buffer[16];
    for( int i = 0; i < ISR_COUNT; i++ )
    {
        sprintf(buffer, "_isr%d", i);
        isrs[i].index = i;
        isrs[i].stub = symbol_find(buffer);
    }
    isrs[ISR_COUNT].index = SYSCALL_VECTOR;
    isrs[ISR_COUNT].stub = symbol_find("_isr127");

    for( int i = 0; i < ISR_COUNT + 1; i++ )
    {
        idt_set_gate(isrs[i].index, isrs[i].stub, 0x01, 0x8E);
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

void fault_handler( struct regs* r )
{
    irq_handler_t handler = isr_routines[r->int_no];
    printf("%s\n", exception_messages[r->int_no]);
    if(handler)
    {
        handler(r);
    }else
    {
        KPANIC("Process caused an unhandled exception", r);
        STOP;
    }
}


