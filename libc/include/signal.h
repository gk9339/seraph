#ifndef _SIGNAL_H
#define _SIGNAL_H

#include <sys/types.h>
#include <sys/signals.h>

#define SIG_DFL ((_sig_func_ptr)0)
#define SIG_IGN ((_sig_func_ptr)1)
#define SIG_ERR ((_sig_func_ptr)-1)

extern const char* sys_siglist[];
extern const char* sys_signame[];

sighandler_t signal(int, sighandler_t);
int raise( int );
int kill( pid_t, int );

#endif
