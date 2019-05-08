.section .symbols

.extern __dso_handle
.type __dso_handle, @function

.extern __DTOR_END__
.type __DTOR_END__, @function

.extern _fini
.type _fini, @function

.extern gdt_flush
.type gdt_flush, @function

.extern gdt_initialize
.type gdt_initialize, @function

.extern gdt_set_gate
.type gdt_set_gate, @function

.extern idt_initialize
.type idt_initialize, @function

.extern idt_load
.type idt_load, @function

.extern idt_set_gate
.type idt_set_gate, @function

.extern _init
.type _init, @function

.extern _kernel_end
.type _kernel_end, @function

.extern kernel_main
.type kernel_main, @function

.extern _kernel_start
.type _kernel_start, @function

.extern kpanic
.type kpanic, @function

.extern memset
.type memset, @function

.extern printf
.type printf, @function

.extern putchar
.type putchar, @function

.extern set_kernel_stack
.type set_kernel_stack, @function

.extern _start
.type _start, @function

.extern strlen
.type strlen, @function

.extern terminal_initialize
.type terminal_initialize, @function

.extern terminal_putchar
.type terminal_putchar, @function

.extern terminal_putentryat
.type terminal_putentryat, @function

.extern terminal_setcolor
.type terminal_setcolor, @function

.extern terminal_write
.type terminal_write, @function

.extern terminal_writestring
.type terminal_writestring, @function

.extern __TMC_END__
.type __TMC_END__, @function

.extern tss_flush
.type tss_flush, @function

.global kernel_symbols_start
kernel_symbols_start:

.long __dso_handle
.asciz "__dso_handle"

.long __DTOR_END__
.asciz "__DTOR_END__"

.long _fini
.asciz "_fini"

.long gdt_flush
.asciz "gdt_flush"

.long gdt_initialize
.asciz "gdt_initialize"

.long gdt_set_gate
.asciz "gdt_set_gate"

.long idt_initialize
.asciz "idt_initialize"

.long idt_load
.asciz "idt_load"

.long idt_set_gate
.asciz "idt_set_gate"

.long _init
.asciz "_init"

.long _kernel_end
.asciz "_kernel_end"

.long kernel_main
.asciz "kernel_main"

.long _kernel_start
.asciz "_kernel_start"

.long kpanic
.asciz "kpanic"

.long memset
.asciz "memset"

.long printf
.asciz "printf"

.long putchar
.asciz "putchar"

.long set_kernel_stack
.asciz "set_kernel_stack"

.long _start
.asciz "_start"

.long strlen
.asciz "strlen"

.long terminal_initialize
.asciz "terminal_initialize"

.long terminal_putchar
.asciz "terminal_putchar"

.long terminal_putentryat
.asciz "terminal_putentryat"

.long terminal_setcolor
.asciz "terminal_setcolor"

.long terminal_write
.asciz "terminal_write"

.long terminal_writestring
.asciz "terminal_writestring"

.long __TMC_END__
.asciz "__TMC_END__"

.long tss_flush
.asciz "tss_flush"

.global kernel_symbols_end
kernel_symbols_end:
