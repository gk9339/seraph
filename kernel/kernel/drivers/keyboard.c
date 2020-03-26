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
    if(inportb(KEY_PENDING) & 0x01)
    {
        write_fs(keyboard_pipe, 0, 1, (uint8_t []){inportb(KEY_DEVICE)});
    }
    
    irq_ack(KEY_IRQ);
    return 1;
}

int keyboard_install( void )
{
    keyboard_pipe = make_pipe(128);
    keyboard_pipe->type = FS_CHARDEVICE;

    vfs_mount("/dev/kbd", keyboard_pipe);

    irq_install_handler(1, keyboard_handler, "ps2 kbd");

    return 0;
}
