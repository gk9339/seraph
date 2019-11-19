#ifndef _SIGNAL_H
#define _SIGNAL_H

#include <sys/types.h>
#include <sys/signals.h>

sighandler_t signal(int, sighandler_t);

#endif
