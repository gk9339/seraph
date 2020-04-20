#ifndef _SETJMP_H
#define _SETJMP_H

#ifdef __cplusplus
extern "C" {
#endif

#define _JBLEN 9

typedef int jmp_buf[_JBLEN];

void longjmp( jmp_buf j, int r );
int setjmp( jmp_buf j );

#ifdef __cplusplus
}
#endif

#endif
