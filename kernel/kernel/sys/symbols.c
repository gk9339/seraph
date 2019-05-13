#include <stddef.h>
#include <string.h>

#include <kernel/symbols.h>
#include <sys/types.h>

extern char kernel_symbols_start[];
extern char kernel_symbols_end[];

typedef struct
{
    uintptr_t addr;
    char name[];
} kernel_symbol_t;

void(* symbol_find(const char* name))(void)
{
    kernel_symbol_t* k = (kernel_symbol_t*)&kernel_symbols_start;

    while( (uintptr_t)k < (uintptr_t)&kernel_symbols_end )
    {
        if(strcmp(k->name, name))
        {
            k = (kernel_symbol_t*)((uintptr_t)k + sizeof(*k) + strlen(k->name) + 1);
            continue;
        }
        return(void(*)(void))k->addr;
    }

    return NULL;
}
