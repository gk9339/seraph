#ifndef _KERNEL_SYMTAB_H
#define _KERNEL_SYMTAB_H

#include <stddef.h>
#include <string.h>
#include <kernel/types.h>

void(* symbol_find(const char*))(void);

#endif
