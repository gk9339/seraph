#include <string.h>
#include <stdio.h>

#include <kernel/irq.h>
#include <kernel/kernel.h>
#include <kernel/symbols.h>
#include <kernel/serial.h>
#include <kernel/idt.h>
#include <sys/types.h>

// Programmable interrupt controller
#define PIC1 0x20
#define PIC1_COMMAND PIC1
#define PIC1_OFFSET 0x20
#define PIC1_DATA (PIC1+1)

#define PIC2 0xA0
#define PIC2_COMMAND PIC2
#define PIC2_OFFSET 0x28
#define PIC2_DATA (PIC2+1)

#define PIC_EOI 0x20

#define ICW1_ICW4 0x01
#define ICW1_INIT 0x10

#define PIC_WAIT()\
    do{\
        asm volatile(               \
                    "jmp 1f\n"      \
                    "1:\n"          \
                    "   jmp 2f\n"   \
                    "2:"            \
                    );              \
    }while(0)

// Interrupt requests
#define IRQ_CHAIN_SIZE 16
#define IRQ_CHAIN_DEPTH 4

// Interrupts
static volatile int sync_depth = 0;

static void(*irqs[IRQ_CHAIN_SIZE])(void);
static irq_handler_chain_t irq_routines[IRQ_CHAIN_SIZE * IRQ_CHAIN_DEPTH] = {NULL};
static char* _irq_handler_descriptions[IRQ_CHAIN_SIZE * IRQ_CHAIN_DEPTH] = {NULL};

void int_disable( void )
{
    // Check if enabled first
    uint32_t flags;
    asm volatile(
                "pushf\n"
                "pop %%eax\n"
                "movl %%eax, %0\n"
                :"=r"(flags)
                :
                :"%eax"
                );

    // Disable interrupts
    asm volatile("cli");

    if( flags & ( 1<<9 ) )
    {
        sync_depth = 1;
    }else
    {
        sync_depth++;
    }
}

void int_resume( void )
{
    // If there is one or no call depth, reenable
    if( sync_depth == 0 || sync_depth == 1 )
    {
        asm volatile("sti");
    }else
    {
        sync_depth--;
    }
}

void int_enable( void )
{
    sync_depth = 0;
    asm volatile("sti");
}

// Used for procfs
char* get_irq_handler( int irq, int chain )
{
    if( irq >= IRQ_CHAIN_SIZE ) return NULL;
    if( chain >= IRQ_CHAIN_DEPTH ) return NULL;
    return _irq_handler_descriptions[IRQ_CHAIN_SIZE * chain + irq];
}

// Installs handler to IRQ number, and sets description
void irq_install_handler( size_t irq, irq_handler_chain_t handler, char* desc )
{
    // Disable all interrupts while changing handlers
    asm volatile("cli");
    for( size_t i = 0; i < IRQ_CHAIN_DEPTH; i++ )
    {
        if( irq_routines[i * IRQ_CHAIN_SIZE + irq] )
            continue;
        irq_routines[i * IRQ_CHAIN_SIZE + irq] = handler;
        _irq_handler_descriptions[i * IRQ_CHAIN_SIZE + irq] = desc;
        break;
    }
    asm volatile("sti");
}

// Sets routine pointer and description to NULL, removing previous handler
void irq_uninstall_handler( size_t irq )
{
    // Disable all interrupts while changing handlers
    asm volatile("cli");
    for( size_t i = 0; i < IRQ_CHAIN_DEPTH; i++ )
    {
        irq_routines[i * IRQ_CHAIN_SIZE + irq] = NULL;
        _irq_handler_descriptions[i * IRQ_CHAIN_SIZE + irq] = NULL;
    }
    asm volatile("sti");
}

// PIC Initialization for IRQs
static void irq_remap( void )
{
    // Send Initialization command to PIC1 and PIC2
    outportb(PIC1_COMMAND, ICW1_INIT|ICW1_ICW4); PIC_WAIT();
    outportb(PIC2_COMMAND, ICW1_INIT|ICW1_ICW4); PIC_WAIT();

    // Remap PIC1 and PIC2 to their offsets
    outportb(PIC1_DATA, PIC1_OFFSET); PIC_WAIT();
    outportb(PIC2_DATA, PIC2_OFFSET); PIC_WAIT();

    // IRQ2 -> connection to slave
    outportb(PIC1_DATA, 0x04); PIC_WAIT();
    outportb(PIC2_DATA, 0x02); PIC_WAIT();

    // Request 8086 mode on each PIC
    outportb(PIC1_DATA, 0x01); PIC_WAIT();
    outportb(PIC2_DATA, 0x01); PIC_WAIT();
}

// Setup IDT gates for IRQs
static void irq_setup_gates( void )
{
    for( size_t i = 0; i < IRQ_CHAIN_SIZE; i++ )
        idt_set_gate((uint8_t)(32 + i), irqs[i], 0x08, 0x8E);
}

// Setup IRQ function table, intialize PICs, install IDT gates
void irq_initialize( void )
{
    char buffer[16];
    memset(&buffer, '\0', sizeof(buffer));
    for( int i = 0; i < IRQ_CHAIN_SIZE; i++ )
    {
        sprintf(buffer, "_irq%d", i);
        irqs[i] = symbol_find(buffer);
    }
    irq_remap();
    irq_setup_gates();

    uint8_t val = inportb(0x4D1);
    outportb(0x4D1, val | ( 1 << ( 10 - 8 )) | ( 1 << ( 11 - 8 )));
}

// ACK to the correct PIC
void irq_ack( size_t irq_no )
{
    if( irq_no >= 8 )
        outportb(PIC2_COMMAND, PIC_EOI);
    outportb(PIC1_COMMAND, PIC_EOI);
}

// Handle IRQ, if withing valid range, and if a handler is installed
void irq_handler( struct regs* r )
{
    // Disable interrupts while handling
    int_disable();
    if( r->int_no < 47 && r->int_no >= 32 )
    {
        for( size_t i = 0; i < IRQ_CHAIN_DEPTH; i++ )
        {
            irq_handler_chain_t handler = irq_routines[i * IRQ_CHAIN_SIZE + (r->int_no - 32)];
            if( !handler ) break;
            if( handler(r) )
            {
                int_resume();
                return;
            }
        }
        irq_ack(r->int_no - 32);
    }
}
