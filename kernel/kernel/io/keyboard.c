#include <stdio.h>

#include <kernel/keyboard.h>
#include <kernel/serial.h>
#include <kernel/irq.h>
#include <kernel/process.h>
#include <kernel/task.h>
#include <kernel/pipe.h>

static fs_node_t* keyboard_pipe;

static int keyboard_handler( struct regs *r __attribute__((unused)) )
{
    unsigned char scancode;
    if(inportb(KEY_PENDING) & 0x01)
    {
        scancode = convert_scancode(inportb(KEY_DEVICE));
        write_fs(keyboard_pipe, 0, 1, (uint8_t []){scancode});
    }
    
    irq_ack(KEY_IRQ);
    return 1;
}

int keyboard_install( void )
{
    keyboard_pipe = make_pipe(128);

    keyboard_pipe->flags = FS_CHARDEVICE;

    vfs_mount("/dev/kbd", keyboard_pipe);

    irq_install_handler(1, keyboard_handler, "ps2 kbd");

    return 0;
}

char convert_scancode( unsigned char scancode )
{
    char c;
    unsigned char kbdus[128] =
    {
        0,  27, '1', '2', '3', '4', '5', '6', '7', '8',	/* 9 */
      '9', '0', '-', '=', '\b',	/* Backspace */
      '\t',			/* Tab */
      'q', 'w', 'e', 'r',	/* 19 */
      't', 'y', 'u', 'i', 'o', 'p', '[', ']', '\n',	/* Enter key */
        0,			/* 29   - Control */
      'a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l', ';',	/* 39 */
     '\'', '`',   0,		/* Left shift */
     '\\', 'z', 'x', 'c', 'v', 'b', 'n',			/* 49 */
      'm', ',', '.', '/',   0,				/* Right shift */
      '*',
        0,	/* Alt */
      ' ',	/* Space bar */
        0,	/* Caps lock */
        0,	/* 59 - F1 key ... > */
        0,   0,   0,   0,   0,   0,   0,   0,
        0,	/* < ... F10 */
        0,	/* 69 - Num lock*/
        0,	/* Scroll Lock */
        0,	/* Home key */
        0,	/* Up Arrow */
        0,	/* Page Up */
      '-',
        0,	/* Left Arrow */
        0,
        0,	/* Right Arrow */
      '+',
        0,	/* 79 - End key*/
        0,	/* Down Arrow */
        0,	/* Page Down */
        0,	/* Insert Key */
        0,	/* Delete Key */
        0,   0,   0,
        0,	/* F11 Key */
        0,	/* F12 Key */
        0,	/* All other keys are undefined */
    };	

    if( scancode <= 128 )
    {
        c = kbdus[scancode];
    }else
    {
        c = '\0';
    }

    return c;
}
