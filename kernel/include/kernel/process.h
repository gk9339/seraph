#ifndef _KERNEL_PROCESS_H
#define _KERNEL_PROCESS_H

#include <kernel/types.h> // struct regs
#include <kernel/mem.h> // page_directory_t
#include <kernel/fs.h> // fs_node_t
#include <tree.h> // tree_t
#include <time.h> // list_t
#include <stdint.h> // intN_t
#include <sys/signals.h> // NUMSIGNALS

#define SIGNAL_RETURN 0x1523A6AA
#define THREAD_RETURN 0x260913A3

#define USER_ROOT_UID (user_t)0

typedef signed int pid_t;
typedef unsigned int user_t;
typedef unsigned int status_t;

extern list_t* process_list;

// Unix waitpid() options
enum wait_option
{
    WCONTINUED,
    WNOHANG,
    WUNTRACED
};

// x86 task
typedef struct thread
{
    uintptr_t esp; // Stack pointer
    uintptr_t ebp; // Base pointer
    uintptr_t eip; // Instruction pointer

    uint8_t fpu_enabled;
    uint8_t fp_regs[512];

    uint8_t padding[32];

    page_directory_t* page_directory;
} thread_t;

typedef struct image
{
    size_t size;
    uintptr_t entry; // Entry point
    uintptr_t heap;
    uintptr_t heap_actual;
    uintptr_t stack;
    uintptr_t user_stack;
    uintptr_t start;
    uintptr_t shm_heap;
    volatile int lock[2];
} image_t;

// Resizable descriptor table
typedef struct descriptor_table
{
    fs_node_t** entries;
    uint64_t* offsets;
    int* flags;
    size_t length;
    size_t capacity;
    size_t refs;
} fd_table_t;

typedef struct signal_table
{
    uintptr_t functions[NUMSIGNALS+1];
} sig_table_t;

// Portable process struct
typedef struct process
{
    pid_t id;           // Process ID
    char* name;         // Process name
    char* description;  // Process description
    user_t user;        // Effective User
    user_t real_user;   // Real user ID
    user_t user_group;  // user gid
    int mask;           // umask

    char** cmdline;

    pid_t group;    // Thread group
    pid_t job;      // Job group
    pid_t session;  // Session group

    thread_t thread;            // Task information
    tree_node_t* tree_entry;    // Process tree entry
    image_t image;              // Memory image information
    fs_node_t* wd_node;         // Working directory pointer
    char* wd_name;              // Working directory name
    fd_table_t* fds;            // File descriptor table
    status_t status;            // Process status
    sig_table_t signals;        // Signal handlers
    uint8_t finished;           // Process finished
    uint8_t started;            // Process started
    uint8_t running;            // Process running
    struct regs* syscall_registers; // Registers at interrupt
    list_t* wait_queue;         // Processes waiting on this process
    list_t* shm_mappings;       // Shared memory mappings
    list_t* signal_queue;       // Queued signals
    thread_t signal_state;
    char* signal_kstack;
    node_t sched_node;
    node_t sleep_node;
    node_t* timed_sleep_node;
    uint8_t is_daemon;
    volatile uint8_t sleep_interrupted;
    list_t* node_waits;
    int awoken_index;
    node_t* timeout_node;
    struct timeval start;
    uint8_t suspended;
} process_t;

typedef struct
{
    unsigned long end_tick;
    unsigned long end_subtick;
    process_t* process;
    int is_fswait;
} sleeper_t;

void initialize_process_tree( void );
process_t* spawn_process( volatile process_t* parent, int reuse_fds );
process_t* spawn_init( void );
process_t* spawn_kidle( void );
void set_process_environment( process_t* proc, page_directory_t* dir );
void make_process_ready( process_t* proc );
uint8_t process_available( void );
process_t* next_ready_process( void );
uint32_t process_append_fd( process_t* proc, fs_node_t* node );
process_t* process_from_pid( pid_t pid );
void delete_process( process_t* proc );
process_t* process_get_parent( process_t* proc );
uint32_t process_move_fd( process_t* proc, int src, int dest );
int wakeup_queue( list_t* queue );
int wakeup_queue_interrupted( list_t* queue );
int process_is_ready( process_t* proc );

void wakeup_sleepers( unsigned long seconds, unsigned long subseconds );
void sleep_until( process_t* process, unsigned long seconds, unsigned long subseconds );
int sleep_on( list_t* queue );

extern volatile process_t* current_process;
extern process_t* kernel_idle_task;
extern tree_t* process_tree;

int process_wait_nodes( process_t* proc, fs_node_t* nodes[], int timeout );
int process_alert_node( process_t* proc, void* value );
int process_awaken_from_fswait( process_t* proc, int index );

typedef void (*daemon_t)( void*, char* );
int create_kernel_daemon( daemon_t daemon, char* name, void* argp );

void cleanup_process( process_t* proc, int retval );
void reap_process( process_t* proc );
int waitpid( int pid, int* status, int options );

int is_valid_process( process_t* proc );

void debug_print_proc_tree( char** );

#endif
