#include <kernel/pty.h>
#include <hashtable.h>
#include <sys/signals.h>
#include <kernel/signal.h>
#include <stdio.h>
#include <errno.h>
#include <stdlib.h>
#include <kernel/cmos.h>
#include <kernel/kernel.h>
#include <kernel/serial.h>

#define PTY_BUFFER_SIZE 4096

#define MIN(A, B) ((A) < (B) ? (A) : (B))

#define PTR_INRANGE(PTR) ((uintptr_t)(PTR) > current_process->image.entry)
#define PTR_VALIDATE(PTR) ptr_validate((void *)(PTR), __func__)

static void ptr_validate( void* ptr, const char* syscall )
{
    if( ptr && !PTR_INRANGE(ptr) )
    {
        char debug_str[256];
        debug_logf(debug_str, "SEGFAULT: invalid pointer passsed to %s (0x%x < 0x%x)", syscall, (uintptr_t)ptr, current_process->image.entry);
        KPANIC("Segmentation fault", NULL);
    }
}

static int _pty_counter = 0;
static hashtable_t* _pty_index = NULL;
static fs_node_t* _pty_dir = NULL;
static fs_node_t* _dev_tty = NULL;

static void pty_write_in( pty_t* pty, uint8_t c )
{
    circular_buffer_write( pty->in, 1, &c );
}

static void pty_write_out( pty_t* pty, uint8_t c )
{
    circular_buffer_write( pty->out, 1, &c );
}

static void dump_input_buffer( pty_t* pty )
{
    char* c = pty->canon_buffer;
    while( pty->canon_buflen )
    {
        pty->write_in(pty, (uint8_t)*c);
        pty->canon_buflen--;
        c++;
    }
}

static void clear_input_buffer( pty_t* pty )
{
    pty->canon_buflen = 0;
    pty->canon_buffer[0] = '\0';
}

void pty_output_process_slave( pty_t* pty, uint8_t c )
{
    if( c == '\n' && (pty->pty_termios.c_oflag & ONLCR) )
    {
        pty->write_out(pty, (uint8_t)c);
        c = '\r';
        pty->write_out(pty, (uint8_t)c);
        return;
    }

    if( c == '\r' && (pty->pty_termios.c_oflag & ONLRET) )
    {
        return;
    }

    if( c >= 'a' && c <= 'z' && (pty->pty_termios.c_oflag & OLCUC) )
    {
        c = (uint8_t)(c + 'a' - 'A');
        pty->write_out(pty, (uint8_t)c);
        return;
    }

    pty->write_out(pty, (uint8_t)c);
}

void pty_output_process( pty_t* pty, uint8_t c )
{
    pty_output_process_slave(pty, c);
}

static int is_control( int c )
{
    return c < ' ' || c == 0x7F;
}

static void erase_one( pty_t* pty, int erase )
{
    if( pty->canon_buflen > 0 )
    {
        int vwidth = 1;
        pty->canon_buflen--;
        if( is_control(pty->canon_buffer[pty->canon_buflen]) )
        {
            vwidth = 2;
        }
        
        pty->canon_buffer[pty->canon_buflen] = '\0';
        if( pty->pty_termios.c_lflag & ECHO )
        {
            if( erase )
            {
                for( int i = 0; i < vwidth; i++ )
                {
                    pty_output_process(pty, '\010');
                    pty_output_process(pty, ' ');
                    pty_output_process(pty, '\010');
                }
            }
        }
    }
}

void pty_input_process( pty_t* pty, uint8_t c )
{
    if( pty->next_is_verbatim )
    {
        pty->next_is_verbatim = 0;
        if( pty->canon_buflen < pty->canon_bufsize )
        {
            pty->canon_buffer[pty->canon_buflen] = c;
            pty->canon_buflen++;
        }
        if( pty->pty_termios.c_lflag & ECHO )
        {
            if( is_control(c) )
            {
                pty_output_process(pty, '^');
                pty_output_process(pty, (uint8_t)('@'+c) % 128);
            }else
            {
                pty_output_process(pty, c);
            }
        }
        return;
    }
    if( pty->pty_termios.c_lflag & ISIG )
    {
        int sig = -1;
        if( c == pty->pty_termios.c_cc[VINTR] )
        {
            sig = SIGINT;
        }else if( c == pty->pty_termios.c_cc[VQUIT] )
        {
            sig = SIGQUIT;
        }else if( c == pty->pty_termios.c_cc[VSUSP] )
        {
            sig = SIGTSTP;
        }

        if( sig != -1 )
        {
            if( pty->pty_termios.c_lflag & ECHO )
            {
                pty_output_process(pty, '^');
                pty_output_process(pty, (uint8_t)('@' + c) % 128);
                pty_output_process(pty, '\n');
            }
            clear_input_buffer(pty);
            if( pty->foreground_process )
            {
                group_send_signal(pty->foreground_process, sig, 1);
            }
            return;
        }
    }

    if( pty->pty_termios.c_iflag & ISTRIP )
    {
        c &= 0x7F;
    }

    if( (pty->pty_termios.c_iflag & IGNCR) && c == '\r' )
    {
        return;
    }

    if( (pty->pty_termios.c_iflag & INLCR) && c == '\n' )
    {
        c = '\r';
    }else if( (pty->pty_termios.c_iflag & ICRNL) && c == '\r' )
    {
        c = '\n';
    }

    if( pty->pty_termios.c_lflag & ICANON )
    {
        if( c == pty->pty_termios.c_cc[VLNEXT] && (pty->pty_termios.c_lflag & IEXTEN) )
        {
            pty->next_is_verbatim = 1;
            pty_output_process(pty, '^');
            pty_output_process(pty, '\010');
            return;
        }

        if( c == pty->pty_termios.c_cc[VKILL] )
        {
            while( pty->canon_buflen > 0 )
            {
                erase_one(pty, pty->pty_termios.c_lflag & ECHOK);
            }
            if( (pty->pty_termios.c_lflag & ECHO) && !(pty->pty_termios.c_lflag & ECHOK) )
            {
                pty_output_process(pty, '^');
                pty_output_process(pty, (uint8_t)('@' + c) % 128);
            }
            return;
        }

        if( c == pty->pty_termios.c_cc[VERASE])
        {
            erase_one(pty, pty->pty_termios.c_lflag & ECHOE);
            if( (pty->pty_termios.c_lflag & ECHO) && !(pty->pty_termios.c_lflag & ECHOE) )
            {
                pty_output_process(pty, '^');
                pty_output_process(pty, (uint8_t)('@' + c) % 128);
            }
            return;
        }

        if( c == pty->pty_termios.c_cc[VWERASE] && (pty->pty_termios.c_lflag & IEXTEN) )
        {
            while( pty->canon_buflen && pty->canon_buffer[pty->canon_buflen - 1] == ' ' )
            {
                erase_one(pty, pty->pty_termios.c_lflag & ECHOE);
            }
            while( pty->canon_buflen && pty->canon_buffer[pty->canon_buflen - 1] != ' ' )
            {
                erase_one(pty, pty->pty_termios.c_lflag & ECHOE);
            }
            if( (pty->pty_termios.c_lflag & ECHO) && !(pty->pty_termios.c_lflag & ECHOE) )
            {
                pty_output_process(pty, '^');
                pty_output_process(pty, (uint8_t)('@' + c) % 128);
            }
            return;
        }

        if( c == pty->pty_termios.c_cc[VEOF] )
        {
            if( pty->canon_buflen )
            {
                dump_input_buffer(pty);
            }else
            {
                circular_buffer_interrupt(pty->in);
            }
            return;
        }

        if( pty->canon_buflen < pty->canon_bufsize )
        {
            pty->canon_buffer[pty->canon_buflen] = c;
            pty->canon_buflen++;
        }
        if( pty->pty_termios.c_lflag & ECHO )
        {
            if( is_control(c) && c != '\n' )
            {
                pty_output_process(pty, '^');
                pty_output_process(pty, (uint8_t)('@' + c) % 128);
            }else
            {
                pty_output_process(pty, c);
            }
        }
        if( c == '\n' || (pty->pty_termios.c_cc[VEOL] && c == pty->pty_termios.c_cc[VEOL]) )
        {
            if( !(pty->pty_termios.c_lflag & ECHO) && (pty->pty_termios.c_lflag & ECHONL) )
            {
                pty_output_process(pty, c);
            }
            pty->canon_buffer[pty->canon_buflen-1] = c;
            dump_input_buffer(pty);
            return;
        }
        return;
    }else if( pty->pty_termios.c_lflag & ECHO )
    {
        pty_output_process(pty, c);
    }

    pty->write_in(pty, (uint8_t)c);
}

static void pty_fill_name( pty_t* pty, char* out )
{
    ((char*)out)[0] = '\0';
    sprintf((char*)out, "/dev/pty/pty%d", pty->name);
}

static int pty_ioctl( pty_t* pty, int request, void* argp )
{
    char debug_str[256];
    switch( request )
    {
        case IOCTLDTYPE:
            return IOCTL_DTYPE_TTY;
        case IOCTLTTYNAME:
            if( !argp ) return -EINVAL;
            PTR_VALIDATE(argp);
            pty->fill_name(pty, argp);
            return 0;
        case IOCTLTTYLOGIN:
            if( current_process->user != 0 ) return -EPERM;
            if( !argp ) return -EINVAL;
            PTR_VALIDATE(argp);
            pty->master->uid = *(int*)argp;
            pty->slave->uid = *(int*)argp;
            return 0;
        case TIOCSWINSZ:
            if( !argp ) return -EINVAL;
            PTR_VALIDATE(argp);
            memcpy(&pty->pty_winsize, argp, sizeof(struct winsize));
            if( pty->foreground_process )
            {
                group_send_signal(pty->foreground_process, SIGWINCH, 1);
            }
            return 0;
        case TIOCGWINSZ:
            if( !argp ) return -EINVAL;
            PTR_VALIDATE(argp);
            memcpy(argp, &pty->pty_winsize, sizeof(struct winsize));
            return 0;
        case TCGETS:
            if( !argp ) return -EINVAL;
            PTR_VALIDATE(argp);
            memcpy(argp, &pty->pty_termios, sizeof(struct termios));
            return 0;
        case TIOCSPGRP:
            if( !argp ) return -EINVAL;
            PTR_VALIDATE(argp);
            pty->foreground_process = *(pid_t*)argp;
            debug_logf(debug_str, "Setting PTY group to %d", pty->foreground_process);
            return 0;
        case TIOCGPGRP:
            if( !argp ) return -EINVAL;
            PTR_VALIDATE(argp);
            *(pid_t*)argp = pty->foreground_process;
            return 0;
        case TCSETS:
        case TCSETSW:
        case TCSETSF:
            if( !argp ) return -EINVAL;
            PTR_VALIDATE(argp);
            if( !(((struct termios*)argp)->c_lflag & ICANON) && (pty->pty_termios.c_lflag & ICANON) )
            {
                dump_input_buffer(pty);
            }
            memcpy(&pty->pty_termios, argp, sizeof(struct termios));
            return 0;
        default:
            return -EINVAL;
    }
}

static uint32_t  read_pty_master( fs_node_t* node, uint32_t offset __attribute__((unused)), uint32_t size, uint8_t* buffer )
{ 
    pty_t* pty = (pty_t*)node->device;

    return circular_buffer_read(pty->out, size, buffer);
}

static uint32_t write_pty_master( fs_node_t* node, uint32_t offset __attribute__((unused)), uint32_t size, uint8_t* buffer )
{
    pty_t* pty = (pty_t*)node->device;

    size_t l = 0;
    for( uint8_t* c = buffer; l < size; c++, l++ )
    {
        pty_input_process(pty, *c);
    }

    return l;
}

static void open_pty_master( fs_node_t* node __attribute__((unused)), unsigned int flags __attribute__((unused)) )
{
    return;
}

static void close_pty_master( fs_node_t* node __attribute__((unused)) )
{
    return;
}

static uint32_t  read_pty_slave( fs_node_t* node, uint32_t offset __attribute__((unused)), uint32_t size, uint8_t* buffer )
{
    pty_t* pty = (pty_t*)node->device;

    if( pty->pty_termios.c_lflag & ICANON )
    {
        return circular_buffer_read(pty->in, size, buffer);
    }else
    {
        if( pty->pty_termios.c_cc[VMIN] == 0 )
        {
            return circular_buffer_read(pty->in, MIN(size, circular_buffer_unread(pty->in)), buffer);
        }else
        {
            return circular_buffer_read(pty->in, MIN(pty->pty_termios.c_cc[VMIN], size), buffer);
        }
    }
}

static uint32_t write_pty_slave( fs_node_t* node, uint32_t offset __attribute__((unused)), uint32_t size, uint8_t* buffer )
{
    pty_t* pty = (pty_t*)node->device;

    size_t l = 0;
    for( uint8_t* c = buffer; l < size; c++, l++ )
    {
        pty_output_process_slave(pty, *c);
    }

    return l;
}

static void open_pty_slave( fs_node_t* node __attribute__((unused)), unsigned int flags __attribute__((unused)) )
{
    return;
}

static void close_pty_slave( fs_node_t* node )
{
    pty_t* pty = (pty_t*)node->device;

    hashtable_remove(_pty_index, (void*)pty->name);

    return;
}

static int ioctl_pty_master( fs_node_t* node, int request, void* argp )
{
    pty_t* pty = (pty_t*)node->device;
    return pty_ioctl(pty, request, argp);
}

static int ioctl_pty_slave( fs_node_t* node, int request, void* argp )
{
    pty_t* pty = (pty_t*)node->device;
    return pty_ioctl(pty, request, argp);
}

static int pty_available_input( fs_node_t* node )
{
    pty_t* pty = (pty_t*)node->device;
    return circular_buffer_unread(pty->in);
}

static int pty_available_output( fs_node_t* node )
{
    pty_t* pty = (pty_t*)node->device;
    return circular_buffer_unread(pty->out);
}

static int check_pty_master( fs_node_t* node )
{
    pty_t* pty = (pty_t*)node->device;
    if( circular_buffer_unread(pty->out) > 0 )
    {
        return 0;
    }
    return 1;
}

static int check_pty_slave( fs_node_t* node )
{
    pty_t* pty = (pty_t*)node->device;
    if( circular_buffer_unread(pty->in) > 0 )
    {
        return 0;
    }
    return 1;
}

static int wait_pty_master( fs_node_t* node, void* process )
{
    pty_t* pty = (pty_t*)node->device;
    circular_buffer_selectwait(pty->out, process);
    return 0;
}

static int wait_pty_slave( fs_node_t* node, void* process )
{
    pty_t* pty = (pty_t*)node->device;
    circular_buffer_selectwait(pty->in, process);
    return 0;
}

static fs_node_t* pty_master_create( pty_t* pty )
{
    fs_node_t* fnode = malloc(sizeof(fs_node_t));
    memset(fnode, 0x00, sizeof(fs_node_t));

    fnode->name[0] = '\0';
    sprintf(fnode->name, "pty master");
    fnode->uid   = current_process->user;
    fnode->gid   = 0;
    fnode->mask  = 0666;
    fnode->type  = FS_PIPE;
    fnode->read  =  read_pty_master;
    fnode->write = write_pty_master;
    fnode->open  =  open_pty_master;
    fnode->close = close_pty_master;
    fnode->selectcheck = check_pty_master;
    fnode->selectwait  = wait_pty_master;
    fnode->readdir = NULL;
    fnode->finddir = NULL;
    fnode->ioctl = ioctl_pty_master;
    fnode->get_size = pty_available_output;
    fnode->ctime   = now();
    fnode->mtime   = now();
    fnode->atime   = now();

    fnode->device = pty;

    return fnode;
}

static fs_node_t* pty_slave_create( pty_t* pty )
{
    fs_node_t* fnode = malloc(sizeof(fs_node_t));
    memset(fnode, 0x00, sizeof(fs_node_t));

    fnode->name[0] = '\0';
    sprintf(fnode->name, "pty slave");
    fnode->uid   = current_process->user;
    fnode->gid   = 0;
    fnode->mask  = 0620;
    fnode->type  = FS_CHARDEVICE;
    fnode->read  =  read_pty_slave;
    fnode->write = write_pty_slave;
    fnode->open  =  open_pty_slave;
    fnode->close = close_pty_slave;
    fnode->selectcheck = check_pty_slave;
    fnode->selectwait  = wait_pty_slave;
    fnode->readdir = NULL;
    fnode->finddir = NULL;
    fnode->ioctl = ioctl_pty_slave;
    fnode->get_size = pty_available_input;
    fnode->ctime   = now();
    fnode->mtime   = now();
    fnode->atime   = now();

    fnode->device = pty;

    return fnode;
}

static int isatty( fs_node_t* node )
{
    if (!node) return 0;
    if (!node->ioctl) return 0;
    return ioctl_fs(node, IOCTLDTYPE, NULL) == IOCTL_DTYPE_TTY;
}

static int readlink_dev_tty( fs_node_t* node __attribute__((unused)), char* buf, size_t size )
{
    pty_t* pty = NULL;

    for( unsigned int i = 0; i < ((current_process->fds->length < 3) ? current_process->fds->length : 3); ++i )
    {
        if( isatty(current_process->fds->entries[i]) )
        {
            pty = (pty_t*)current_process->fds->entries[i]->device;
            break;
        }
    }

    char tmp[30];
    size_t req;
    if( !pty )
    {
        sprintf(tmp, "/dev/null");
    }else
    {
        pty->fill_name(pty, tmp);
    }

    req = strlen(tmp) + 1;

    if( size < req )
    {
        memcpy(buf, tmp, size);
        buf[size-1] = '\0';
        return size-1;
    }

    if( size > req ) size = req;

    memcpy(buf, tmp, size);
    return size-1;
}

static fs_node_t* create_dev_tty( void )
{
    fs_node_t* fnode = malloc(sizeof(fs_node_t));
    memset(fnode, 0x00, sizeof(fs_node_t));

    fnode->inode = 0;
    strcpy(fnode->name, "tty");
    fnode->mask = 0777;
    fnode->uid  = 0;
    fnode->gid  = 0;
    fnode->type = FS_FILE | FS_SYMLINK;
    fnode->readlink = readlink_dev_tty;
    fnode->length  = 1;
    fnode->nlink   = 1;
    fnode->ctime   = now();
    fnode->mtime   = now();
    fnode->atime   = now();
    return fnode;
}

static struct dirent* readdir_pty( fs_node_t* fnode __attribute__((unused)), uint32_t index )
{
    if( index == 0 )
    {
        struct dirent* out = malloc(sizeof(struct dirent));
        memset(out, 0x00, sizeof(struct dirent));
        out->ino = 0;
        strcpy(out->name, ".");
        return out;
    }

    if( index == 1 )
    {
        struct dirent* out = malloc(sizeof(struct dirent));
        memset(out, 0x00, sizeof(struct dirent));
        out->ino = 0;
        strcpy(out->name, "..");
        return out;
    }

    index -= 2;

    pty_t* out_pty = NULL;
    list_t* values = hashtable_values(_pty_index);
    foreach( node, values )
    {
        if( index == 0 )
        {
            out_pty = node->value;
            break;
        }
        index--;
    }
    list_free(values);

    if( out_pty )
    {
        struct dirent* out = malloc(sizeof(struct dirent));
        memset(out, 0x00, sizeof(struct dirent));
        out->ino = out_pty->name;
        out->name[0] = '\0';
        sprintf(out->name, "%d", out_pty->name);
        return out;
    }else
    {
        return NULL;
    }
}

static fs_node_t* finddir_pty( fs_node_t* node __attribute__((unused)), char* name )
{
    if( !name ) return NULL;
    if( strlen(name) < 1 ) return NULL;

    int c = 0;
    for( int i = 0; name[i]; ++i )
    {
        if( name[i] < '0' || name[i] > '9' )
        {
            return NULL;
        }
        c = c * 10 + name[i] - '0';
    }

    pty_t* _pty = hashtable_get(_pty_index, (void*)c);

    if( !_pty )
    {
        return NULL;
    }

    return _pty->slave;
}

static fs_node_t* create_pty_dir( void )
{
    fs_node_t * fnode = malloc(sizeof(fs_node_t));
    memset(fnode, 0x00, sizeof(fs_node_t));

    fnode->inode = 0;
    strcpy(fnode->name, "pty");
    fnode->mask = 0555;
    fnode->uid  = 0;
    fnode->gid  = 0;
    fnode->type = FS_DIRECTORY;
    fnode->read    = NULL;
    fnode->write   = NULL;
    fnode->open    = NULL;
    fnode->close   = NULL;
    fnode->readdir = readdir_pty;
    fnode->finddir = finddir_pty;
    fnode->nlink   = 1;
    fnode->ctime   = now();
    fnode->mtime   = now();
    fnode->atime   = now();
    return fnode;
}

static void pty_initialize( void )
{
    _pty_index = hashtable_create_int(10);
    _pty_dir = create_pty_dir();
    _dev_tty = create_dev_tty();

    vfs_mount("/dev/pty", _pty_dir);
    vfs_mount("/dev/tty", _dev_tty);
}

pty_t* pty_new( struct termios* pty_termios, struct winsize* pty_winsize )
{
    if (!_pty_index) {
        pty_initialize();
    }

    pty_t* pty = malloc(sizeof(pty_t));

    pty->next_is_verbatim = 0;

    /* stdin linkage; characters from terminal → PTY slave */
    pty->in  = circular_buffer_create(PTY_BUFFER_SIZE);
    pty->out = circular_buffer_create(PTY_BUFFER_SIZE);

    pty->in->discard = 1;

    /* Master endpoint - writes go to stdin, reads come from stdout */
    pty->master = pty_master_create(pty);

    /* Slave endpoint, reads come from stdin, writes go to stdout */
    pty->slave  = pty_slave_create(pty);

    /* pty name */
    pty->name   = _pty_counter++;
    pty->fill_name = pty_fill_name;

    pty->write_in = pty_write_in;
    pty->write_out = pty_write_out;

    hashtable_set(_pty_index, (void*)pty->name, pty);

    if( pty_winsize )
    {
        memcpy(&pty->pty_winsize, pty_winsize, sizeof(struct winsize));
    }else
    {
        /* Sane defaults */
        pty->pty_winsize.ws_row = 25;
        pty->pty_winsize.ws_col = 80;
    }

    /* Controlling and foreground processes are set to 0 by default */
    pty->controlling_process = 0;
    pty->foreground_process = 0;

    if( pty_termios )
    {
        memcpy(&pty->pty_termios, pty_termios, sizeof(struct termios));
    }else
    {   
        /* Sane defaults */
        pty->pty_termios.c_iflag = ICRNL | BRKINT;
        pty->pty_termios.c_oflag = ONLCR | OPOST;
        pty->pty_termios.c_lflag = ECHO | ECHOE | ECHOK | ICANON | ISIG | IEXTEN;
        pty->pty_termios.c_cflag = CREAD | CS8;
        pty->pty_termios.c_cc[VEOF]   =  4; /* ^D */
        pty->pty_termios.c_cc[VEOL]   =  0; /* Not set */
        pty->pty_termios.c_cc[VERASE] = 0x7f; /* ^? */
        pty->pty_termios.c_cc[VINTR]  =  3; /* ^C */
        pty->pty_termios.c_cc[VKILL]  = 21; /* ^U */
        pty->pty_termios.c_cc[VMIN]   =  1;
        pty->pty_termios.c_cc[VQUIT]  = 28; /* ^\ */
        pty->pty_termios.c_cc[VSTART] = 17; /* ^Q */
        pty->pty_termios.c_cc[VSTOP]  = 19; /* ^S */
        pty->pty_termios.c_cc[VSUSP] = 26; /* ^Z */
        pty->pty_termios.c_cc[VTIME]  =  0;
        pty->pty_termios.c_cc[VLNEXT] = 22; /* ^V */
        pty->pty_termios.c_cc[VWERASE] = 23; /* ^W */   
    }

    pty->canon_buffer  = malloc(PTY_BUFFER_SIZE);
    pty->canon_bufsize = PTY_BUFFER_SIZE-2;
    pty->canon_buflen  = 0;

    char path[32];
    pty->fill_name(pty, path);
    vfs_mount(path, pty->slave);

    return pty;
}

int pty_create( void* termios, void* winsize, fs_node_t** fs_master, fs_node_t** fs_slave )
{
    pty_t* pty = pty_new(termios, winsize);

    *fs_master = pty->master;
    *fs_slave = pty->slave;

    return 0;
}
