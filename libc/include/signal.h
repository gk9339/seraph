#ifndef _SIGNAL_H
#define _SIGNAL_H

#ifdef __cplusplus
extern "C" {
#endif

#include <sys/types.h>
#include <sys/signals.h>

#define SIG_DFL ((sighandler_t)0)
#define SIG_IGN ((sighandler_t)1)
#define SIG_ERR ((sighandler_t)-1)

typedef int sig_atomic_t;

extern const char* sys_siglist[];
extern const char* sys_signame[];

sighandler_t signal(int, sighandler_t);
int raise( int );
int kill( pid_t, int );

#ifdef __cplusplus
}
#endif

#endif
