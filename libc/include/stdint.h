#ifndef _STDINT_H
#define _STDINT_H

#ifdef __cplusplus
extern "C" {
#endif

#define UINT8_MAX   0xff 
#define UINT16_MAX  0xffff 
#define UINT32_MAX  0xffffffff 
#define UINT64_MAX  0xffffffffffffffff 

#define INT8_MAX    0x7f 
#define INT16_MAX   0x7fff 
#define INT32_MAX   0x7fffffff 
#define INT64_MAX   0x7fffffffffffffff 

#define INT8_MIN    (-0x7f - 1)
#define INT16_MIN   (-0x7fff - 1)
#define INT32_MIN   (-0x7fffffff - 1)
#define INT64_MIN   (-0x7fffffffffffffff - 1)

#define UINTPTR_MAX       0xffffffff
#define INTPTR_MIN        (-0x7fffffff - 1)
#define INTPTR_MAX        0x7fffffff
#define PTRDIFF_MIN  INT32_MIN
#define PTRDIFF_MAX  INT32_MAX

#define INTMAX_MIN        (-0x7fffffffffffffff - 1)
#define INTMAX_MAX        0x7fffffffffffffff
#define UINTMAX_MAX       0xffffffffffffffff

#define INT8_C(x)    (x)
#define INT16_C(x)   (x)
#define INT32_C(x)   ((x) + (INT32_MAX - INT32_MAX))
#define INT64_C(x)   ((x) + (INT64_MAX - INT64_MAX))

#define INTMAX_C(x)  ((x) + (INT64_MAX - INT64_MAX))
#define UINTMAX_C(x) ((x) + (UINT64_MAX - UINT64_MAX))

typedef unsigned char uint8_t;
typedef unsigned short uint16_t;
typedef unsigned long uint32_t;
typedef unsigned long long uint64_t;

typedef signed char int8_t;
typedef signed short int16_t;
typedef signed long int32_t;
typedef signed long long int64_t;

typedef unsigned long uintptr_t;
typedef signed long intptr_t;
typedef signed long ptrdiff_t;

typedef unsigned long long uintmax_t;
typedef signed long long intmax_t;

typedef signed char int_fast8_t;
typedef signed short int_fast16_t;
typedef signed long int_fast32_t;
typedef signed long int_fast64_t;

typedef unsigned char uint_fast8_t;
typedef unsigned short uint_fast16_t;
typedef unsigned long uint_fast32_t;
typedef unsigned long uint_fast64_t;

typedef signed char int_least8_t;
typedef signed short int_least16_t;
typedef signed long int_least32_t;
typedef signed long int_least64_t;

typedef unsigned char uint_least8_t;
typedef unsigned short uint_least16_t;
typedef unsigned long uint_least32_t;
typedef unsigned long uint_least64_t;

typedef unsigned int wint_t;

#ifdef __cplusplus
}
#endif

#endif
