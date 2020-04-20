#ifndef _ASSERT_H
#define _ASSERT_H

#ifdef __cplusplus
extern "C" {
#endif

extern void __assert_func( const char* file, int line, const char* func, const char* failedexpr);
#define assert(statement) ((statement) ? (void)0 : __assert_func(__FILE__, __LINE__, __FUNCTION__, #statement))

#ifdef __cplusplus
}
#endif

#endif
