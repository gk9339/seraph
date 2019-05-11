#include <stdio.h>
#include <string.h>

#include <kernel/irq.h>
#include <kernel/idt.h>
#include <kernel/serial.h>
#include <kernel/symbols.h>

/* Programmable interrupt controller */
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
        asm volatile("jmp 1f\n\t"   \
                "1:\n\t"            \
                "   jmp 2f\n\t"     \
                "2:");\
    }while(0)

/* Interrupts */
static volatile int sync_depth = 0;

#define SYNC_CLI() asm volatile("cli");
#define SYNC_STI() asm volatile("sti");

/* Interrupt requests */
#define IRQ_CHAIN_SIZE 16
#define IRQ_CHAIN_DEPTH 4

static void(*irqs[IRQ_CHAIN_SIZE])(void);
static irq_handler_chain_t irq_routines[IRQ_CHAIN_SIZE * IRQ_CHAIN_DEPTH] = {NULL};
static char* _irq_handler_descriptions[IRQ_CHAIN_SIZE * IRQ_CHAIN_DEPTH] = {NULL};

void int_disable( void )
{
    /* Check if enabled first */
    uint32_t flags;
    asm volatile("pushf\n\t"
            "pop %%eax\n\t"
            "movl %%eax, %0\n\t"
            :"=r"(flags)
            :
            :"%eax");

    /* Disable interrupts */
    SYNC_CLI();

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
    /* If there is one or no call depth, reenable */
    if( sync_depth == 0 || sync_depth == 1 )
    {
        SYNC_STI();
    }else
    {
        sync_depth--;
    }
}

void int_enable( void )
{
    sync_depth = 0;
    SYNC_STI();
}

char* get_irq_handler( int irq, int chain )
{
    if( irq >= IRQ_CHAIN_SIZE ) return NULL;
    if( chain >= IRQ_CHAIN_DEPTH ) return NULL;
    return _irq_handler_descriptions[IRQ_CHAIN_SIZE * chain + irq];
}

void irq_install_handler( size_t irq, irq_handler_chain_t handler, char* desc )
{
    /* Disable all interrupts while changing handlers */
    SYNC_CLI();
    for( size_t i = 0; i < IRQ_CHAIN_DEPTH; i++ )
    {
        if( irq_routines[i * IRQ_CHAIN_SIZE + irq] )
            continue;
        irq_routines[i * IRQ_CHAIN_SIZE + irq] = handler;
        _irq_handler_descriptions[i * IRQ_CHAIN_SIZE + irq] = desc;
        break;
    }
    SYNC_STI();
}

void irq_uninstall_handler( size_t irq )
{
    /* Disable all interrupts while changing handlers */
    SYNC_CLI();
    for( size_t i = 0; i < IRQ_CHAIN_DEPTH; i++ )
        irq_routines[i * IRQ_CHAIN_SIZE + irq] = NULL;
    SYNC_STI();
}

static void irq_remap( void )
{
    /* Cascade initialization */
    outportb(PIC1_COMMAND, ICW1_INIT|ICW1_ICW4); PIC_WAIT();
    outportb(PIC2_COMMAND, ICW1_INIT|ICW1_ICW4); PIC_WAIT();

    /* Remap */
    outportb(PIC1_DATA, PIC1_OFFSET); PIC_WAIT();
    outportb(PIC2_DATA, PIC2_OFFSET); PIC_WAIT();

    outportb(PIC1_DATA, 0x04); PIC_WAIT();
    outportb(PIC2_DATA, 0x02); PIC_WAIT();

    /* Request 8086 mode on each PIC */
    outportb(PIC1_DATA, 0x01); PIC_WAIT();
    outportb(PIC2_DATA, 0x01); PIC_WAIT();
}

static void irq_setup_gates( void )
{
    for( size_t i = 0; i < IRQ_CHAIN_SIZE; i++ )
        idt_set_gate(32 + i, irqs[i], 0x08, 0x8E);
}

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

void irq_ack( size_t irq_no )
{
    if( irq_no >= 8 )
        outportb(PIC2_COMMAND, PIC_EOI);
    outportb(PIC1_COMMAND, PIC_EOI);
}

void irq_handler( struct regs* r )
{
    /* Disable interrupts while handling */
    int_disable();
    if( r->int_no < 47 && r->int_no >= 32 )
    {
        for( size_t i = 0; i < IRQ_CHAIN_DEPTH; i++ )
        {
            irq_handler_chain_t handler = irq_routines[i * IRQ_CHAIN_SIZE + (r->int_no - 32)];
            if( !handler ) break;
            if( handler(r) )
            {
                goto done;
            }
        }
        irq_ack(r->int_no - 32);
    }
done:
    int_resume();
}
