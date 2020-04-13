#ifndef _SYSCALL_H
#define _SYSCALL_H

#include <stddef.h>
#include <stdint.h>
#include <signal.h>
#include <stdarg.h>
#include <sys/types.h>

#define SYS_EXT 0
#define SYS_GETEUID 1
#define SYS_OPEN 2
#define SYS_READ 3
#define SYS_WRITE 4
#define SYS_CLOSE 5
#define SYS_GETTIMEOFDAY 6
#define SYS_EXECVE 7
#define SYS_FORK 8
#define SYS_GETPID 9
#define SYS_SBRK 10
#define SYS_UNAME 12
#define SYS_OPENPTY 13
#define SYS_SEEK 14
#define SYS_STAT 15
#define SYS_MKPIPE 21
#define SYS_DUP2 22
#define SYS_GETUID 23
#define SYS_SETUID 24
#define SYS_REBOOT 26
#define SYS_READDIR 27
#define SYS_CHDIR 28
#define SYS_GETCWD 29
#define SYS_CLONE 30
#define SYS_SETHOSTNAME 31
#define SYS_GETHOSTNAME 32
#define SYS_MKDIR 34
#define SYS_SHM_OBTAIN 35
#define SYS_SHM_RELEASE 36
#define SYS_KILL 37
#define SYS_SIGNAL 38
#define SYS_GETTID 41
#define SYS_YIELD 42
#define SYS_SYSFUNC 43
#define SYS_SLEEPABS 45
#define SYS_SLEEP 46
#define SYS_IOCTL 47
#define SYS_ACCESS 48
#define SYS_STATF 49
#define SYS_CHMOD 50
#define SYS_UMASK 51
#define SYS_UNLINK 52
#define SYS_WAITPID 53
#define SYS_PIPE 54
#define SYS_MOUNT 55
#define SYS_SYMLINK 56
#define SYS_READLINK 57
#define SYS_LSTAT 58
#define SYS_FSWAIT 59
#define SYS_FSWAIT2 60
#define SYS_CHOWN 61
#define SYS_SETSID 62
#define SYS_SETPGID 63
#define SYS_GETPGID 64
#define SYS_SETHEAP 65
#define SYS_MMAP 66
#define SYS_GETPPID 67
#define SYS_GETGID 68
#define SYS_FCNTL 69
#define SYS_DEBUGVFSTREE 100
#define SYS_DEBUGPROCTREE 101
#define SYS_DEBUGPRINT 102

#define DECL_SYSCALL0(fn)                int syscall_##fn( void )
#define DECL_SYSCALL1(fn,p1)             int syscall_##fn(p1)
#define DECL_SYSCALL2(fn,p1,p2)          int syscall_##fn(p1,p2)
#define DECL_SYSCALL3(fn,p1,p2,p3)       int syscall_##fn(p1,p2,p3)
#define DECL_SYSCALL4(fn,p1,p2,p3,p4)    int syscall_##fn(p1,p2,p3,p4)
#define DECL_SYSCALL5(fn,p1,p2,p3,p4,p5) int syscall_##fn(p1,p2,p3,p4,p5)

#define DEFN_SYSCALL0(fn, num) \
	int syscall_##fn() { \
		int a; __asm__ __volatile__("movl %1,%%eax; int $0x7F" : "=a" (a) : "0" (num)); \
		return a; \
	}

#define DEFN_SYSCALL1(fn, num, P1) \
	int syscall_##fn(P1 p1) { \
		int __res; __asm__ __volatile__("push %%ebx; movl %2,%%ebx; int $0x7F; pop %%ebx" \
				: "=a" (__res) \
				: "0" (num), "r" ((int)(p1))); \
		return __res; \
	}

#define DEFN_SYSCALL2(fn, num, P1, P2) \
	int syscall_##fn(P1 p1, P2 p2) { \
		int __res; __asm__ __volatile__("push %%ebx; movl %2,%%ebx; int $0x7F; pop %%ebx" \
				: "=a" (__res) \
				: "a" (num), "r" ((int)(p1)), "c"((int)(p2))); \
		return __res; \
	}

#define DEFN_SYSCALL3(fn, num, P1, P2, P3) \
	int syscall_##fn(P1 p1, P2 p2, P3 p3) { \
		int __res; __asm__ __volatile__("push %%ebx; movl %2,%%ebx; int $0x7F; pop %%ebx" \
				: "=a" (__res) \
				: "a" (num), "r" ((int)(p1)), "c"((int)(p2)), "d"((int)(p3))); \
		return __res; \
	}

#define DEFN_SYSCALL4(fn, num, P1, P2, P3, P4) \
	int syscall_##fn(P1 p1, P2 p2, P3 p3, P4 p4) { \
		int __res; __asm__ __volatile__("push %%ebx; movl %2,%%ebx; int $0x7F; pop %%ebx" \
				: "=a" (__res) \
				: "a" (num), "r" ((int)(p1)), "c"((int)(p2)), "d"((int)(p3)), "S"((int)(p4))); \
		return __res; \
	}

#define DEFN_SYSCALL5(fn, num, P1, P2, P3, P4, P5) \
	int syscall_##fn(P1 p1, P2 p2, P3 p3, P4 p4, P5 p5) { \
		int __res; __asm__ __volatile__("push %%ebx; movl %2,%%ebx; int $0x7F; pop %%ebx" \
				: "=a" (__res) \
				: "a" (num), "r" ((int)(p1)), "c"((int)(p2)), "d"((int)(p3)), "S"((int)(p4)), "D"((int)(p5))); \
		return __res; \
}

DECL_SYSCALL1(exit, int);
DECL_SYSCALL3(open, const char *, int, int);
DECL_SYSCALL3(read, int, char *, int);
DECL_SYSCALL3(write, int, char *, int);
DECL_SYSCALL1(close, int);
DECL_SYSCALL2(gettimeofday, void*, void*);
DECL_SYSCALL3(execve, const char*, char* const*, char* const*);
DECL_SYSCALL0(fork);
DECL_SYSCALL0(getpid);
DECL_SYSCALL0(getgid);
DECL_SYSCALL0(getppid);
DECL_SYSCALL1(sbrk, int);
DECL_SYSCALL1(uname, void*);
DECL_SYSCALL2(kill, pid_t, int);
DECL_SYSCALL2(signal, uint32_t, sighandler_t);
DECL_SYSCALL5(openpty, int*, int*, char*, void*, void*);
DECL_SYSCALL3(lseek, int, int, int);
DECL_SYSCALL3(readlink, char*, char*, size_t);
DECL_SYSCALL2(stat, char*, void*);
DECL_SYSCALL2(lstat, char*, void*);
DECL_SYSCALL2(fstat, int, void*);
DECL_SYSCALL2(dup2, int, int);
DECL_SYSCALL0(getuid);
DECL_SYSCALL1(reboot, int);
DECL_SYSCALL3(readdir, int, int, void*);
DECL_SYSCALL2(gethostname, char*, size_t);
DECL_SYSCALL2(sethostname, char*, size_t);
DECL_SYSCALL1(chdir, char*);
DECL_SYSCALL2(getcwd, char*, size_t);
DECL_SYSCALL1(setuid, unsigned int);
DECL_SYSCALL0(yield);
DECL_SYSCALL2(nanosleep, unsigned long, unsigned long);
DECL_SYSCALL3(ioctl, int, int, void*);
DECL_SYSCALL2(fswait, int, int*);
DECL_SYSCALL3(fswait2, int, int*, int);
DECL_SYSCALL3(waitpid, int, int*, int);
DECL_SYSCALL1(pipe, int*);
DECL_SYSCALL0(setsid);
DECL_SYSCALL2(setpgid, int, int);
DECL_SYSCALL1(getpgid, int);
DECL_SYSCALL1(setheap, uintptr_t);
DECL_SYSCALL3(fcntl, int, int, va_list);
DECL_SYSCALL2(mmap, uintptr_t, size_t);
DECL_SYSCALL1(debugvfstree, char**);
DECL_SYSCALL1(debugproctree, char**);
DECL_SYSCALL1(debugprint, char*);

#endif
