#ifndef _CDEF_H
#define _CDEF_H

#define _U  01
#define _L  02
#define _N  04
#define _S  010
#define _P  020
#define _C  040
#define _X  0100
#define _B  0200

#ifdef __cplusplus
extern "C" {
#endif

int isdigit( int );
int isspace( int );
int isalnum( int );
int isalpha( int );
int islower( int );
int isprint( int );
int isgraph( int );
int iscntrl( int );
int ispunct( int );
int isupper( int );
int isxdigit( int );
int isascii( int );

int tolower( int );
int toupper( int );

extern unsigned char _ctype_[256];

#ifdef __cplusplus
}
#endif

#endif
