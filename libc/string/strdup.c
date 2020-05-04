#include <string.h>
#include <stdlib.h>

#ifdef __is_libk
#include <kernel/mem.h>
#undef malloc
#define malloc (void*)kmalloc
#endif

// duplicate a string
char* strdup( const char* s )
{
    size_t len = strlen(s);
 
    return memcpy(malloc(len+1), s, len+1);
}
