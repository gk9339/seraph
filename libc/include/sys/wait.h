#ifndef _WAIT_H
#define _WAIT_H

#ifdef __cplusplus
extern "C" {
#endif

#define WNOHANG   0x0001
#define WUNTRACED 0x0002
#define WSTOPPED  0x0004
#define WNOKERN   0x0010

#define WIFEXITED(w)    (((w) & 0xff) == 0)
#define WIFSIGNALED(w)  (((w) & 0x7f) > 0 && (((w) & 0x7f) < 0x7f))
#define WIFSTOPPED(w)   (((w) & 0xff) == 0x7f)
#define WEXITSTATUS(w)  (((w) >> 8) & 0xff)
#define WTERMSIG(w) ((w) & 0x7f)
#define WSTOPSIG    WEXITSTATUS

int waitpid( int, int*, int );
int wait( int* );

#ifdef __cplusplus
}
#endif

#endif
