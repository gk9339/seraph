#include <string.h>
#include <sys/signals.h>

const char* sys_siglist[NSIG] =
{
    [SIGHUP] = "Hangup",
    [SIGINT] = "Interrupt",
    [SIGQUIT] = "Quit",
    [SIGILL] = "Illegal instruction",
    [SIGTRAP] = "Trace/ breakpoint trap",
    [SIGABRT] = "Aborted",
    [SIGEMT] = "Emulation trap",
    [SIGFPE] = "Floating point exception",
    [SIGKILL] = "Killed",
    [SIGBUS] = "Bus error",
    [SIGSEGV] = "Segmentation fault",
    [SIGSYS] = "Bad syscall",
    [SIGPIPE] = "Broken pipe",
    [SIGALRM] = "Alarm clock",
    [SIGTERM] = "Terminated",
    [SIGCHLD] = "Child exited",
    [SIGPWR] = "Power failure",
    [SIGWINCH] = "Window changed size",
    [SIGURG] = "Urgent I/O condition",
    [SIGPOLL] = "I/O possible",
    [SIGSTOP] = "Stopped (signal)",
    [SIGTSTP] = "Stopped", 
    [SIGCONT] = "Continued",
    [SIGTTIN] = "Stopped (tty input)",
    [SIGTTOU] = "Stopped (tty output)",
    [SIGVTALRM] = "Virtual timer expired",
    [SIGPROF] = "PRofiling timer expired",
    [SIGXCPU] = "CPU time limit exceeded",
    [SIGXFSZ] = "File size limit exceeded",
};

const char* sys_signame[NSIG] =
{
    [SIGHUP] = "SIGHUP",
    [SIGINT] = "SIGINT",
    [SIGQUIT] = "SIGQUIT",
    [SIGILL] = "SIGILL",
    [SIGTRAP] = "SIGTRAP",
    [SIGABRT] = "SIGABRT",
    [SIGEMT] = "SIGEMT",
    [SIGFPE] = "SIGFPE",
    [SIGKILL] = "SIGKILL",
    [SIGBUS] = "SIGBUS",
    [SIGSEGV] = "SIGSEGV",
    [SIGSYS] = "SIGSYS",
    [SIGPIPE] = "SIGPIPE",
    [SIGALRM] = "SIGALRM",
    [SIGTERM] = "SIGTERM",
    [SIGUSR1] = "SIGUSR1",
    [SIGUSR2] = "SIGUSR2",
    [SIGCHLD] = "SIGCHLD",
    [SIGPWR] = "SIGPWR",
    [SIGWINCH] = "SIGWINCH",
    [SIGURG] = "SIGURG",
    [SIGPOLL] = "SIGPOLL",
    [SIGSTOP] = "SIGSTOP",
    [SIGTSTP] = "SIGTSTP",
    [SIGCONT] = "SIGCONT",
    [SIGTTIN] = "SIGTTIN",
    [SIGTTOU] = "SIGTTOU",
    [SIGVTALRM] = "SIGVTALRM",
    [SIGPROF] = "SIGPROF",
    [SIGXCPU] = "SIGXCPU",
    [SIGXFSZ] = "SIGXFSZ",
};

char* strsignal( int sig )
{
    if( sig > NSIG ) return "unknown signal";
    if( !sys_siglist[sig] ) return "unknown signal";

    return sys_siglist[sig];
}
