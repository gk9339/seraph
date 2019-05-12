.section .symbols

.extern alloc_frame
.type alloc_frame, @function

.extern bss
.type bss, @function

.extern calloc
.type calloc, @function

.extern clear_frame
.type clear_frame, @function

.extern clone_directory
.type clone_directory, @function

.extern clone_table
.type clone_table, @function

.extern code
.type code, @function

.extern convert_scancode
.type convert_scancode, @function

.extern copy_page_physical
.type copy_page_physical, @function

.extern current_directory
.type current_directory, @function

.extern data
.type data, @function

.extern dma_frame
.type dma_frame, @function

.extern __dso_handle
.type __dso_handle, @function

.extern __DTOR_END__
.type __DTOR_END__, @function

.extern fault_handler
.type fault_handler, @function

.extern _fini
.type _fini, @function

.extern first_frame
.type first_frame, @function

.extern first_n_frames
.type first_n_frames, @function

.extern frames
.type frames, @function

.extern free
.type free, @function

.extern free_frame
.type free_frame, @function

.extern gdt_flush
.type gdt_flush, @function

.extern gdt_initialize
.type gdt_initialize, @function

.extern gdt_set_gate
.type gdt_set_gate, @function

.extern get_irq_handler
.type get_irq_handler, @function

.extern get_page
.type get_page, @function

.extern heap_end
.type heap_end, @function

.extern heap_install
.type heap_install, @function

.extern idt_initialize
.type idt_initialize, @function

.extern idt_load
.type idt_load, @function

.extern idt_set_gate
.type idt_set_gate, @function

.extern _init
.type _init, @function

.extern int_disable
.type int_disable, @function

.extern int_enable
.type int_enable, @function

.extern int_resume
.type int_resume, @function

.extern invalidate_page_tables
.type invalidate_page_tables, @function

.extern invalitate_tables_at
.type invalitate_tables_at, @function

.extern _irq0
.type _irq0, @function

.extern _irq1
.type _irq1, @function

.extern _irq10
.type _irq10, @function

.extern _irq11
.type _irq11, @function

.extern _irq12
.type _irq12, @function

.extern _irq13
.type _irq13, @function

.extern _irq14
.type _irq14, @function

.extern _irq15
.type _irq15, @function

.extern _irq2
.type _irq2, @function

.extern _irq3
.type _irq3, @function

.extern _irq4
.type _irq4, @function

.extern _irq5
.type _irq5, @function

.extern _irq6
.type _irq6, @function

.extern _irq7
.type _irq7, @function

.extern _irq8
.type _irq8, @function

.extern _irq9
.type _irq9, @function

.extern irq_ack
.type irq_ack, @function

.extern irq_handler
.type irq_handler, @function

.extern irq_initialize
.type irq_initialize, @function

.extern irq_install_handler
.type irq_install_handler, @function

.extern irq_uninstall_handler
.type irq_uninstall_handler, @function

.extern _isr0
.type _isr0, @function

.extern _isr1
.type _isr1, @function

.extern _isr10
.type _isr10, @function

.extern _isr11
.type _isr11, @function

.extern _isr12
.type _isr12, @function

.extern _isr127
.type _isr127, @function

.extern _isr13
.type _isr13, @function

.extern _isr14
.type _isr14, @function

.extern _isr15
.type _isr15, @function

.extern _isr16
.type _isr16, @function

.extern _isr17
.type _isr17, @function

.extern _isr18
.type _isr18, @function

.extern _isr19
.type _isr19, @function

.extern _isr2
.type _isr2, @function

.extern _isr20
.type _isr20, @function

.extern _isr21
.type _isr21, @function

.extern _isr22
.type _isr22, @function

.extern _isr23
.type _isr23, @function

.extern _isr24
.type _isr24, @function

.extern _isr25
.type _isr25, @function

.extern _isr26
.type _isr26, @function

.extern _isr27
.type _isr27, @function

.extern _isr28
.type _isr28, @function

.extern _isr29
.type _isr29, @function

.extern _isr3
.type _isr3, @function

.extern _isr30
.type _isr30, @function

.extern _isr31
.type _isr31, @function

.extern _isr4
.type _isr4, @function

.extern _isr5
.type _isr5, @function

.extern _isr6
.type _isr6, @function

.extern _isr7
.type _isr7, @function

.extern _isr8
.type _isr8, @function

.extern _isr9
.type _isr9, @function

.extern isr_initialize
.type isr_initialize, @function

.extern isr_install_handler
.type isr_install_handler, @function

.extern isr_uninstall_handler
.type isr_uninstall_handler, @function

.extern kernel_directory
.type kernel_directory, @function

.extern _kernel_end
.type _kernel_end, @function

.extern kernel_heap_alloc_point
.type kernel_heap_alloc_point, @function

.extern kernel_main
.type kernel_main, @function

.extern _kernel_start
.type _kernel_start, @function

.extern kernel_symbols_end
.type kernel_symbols_end, @function

.extern kernel_symbols_start
.type kernel_symbols_start, @function

.extern keyboard_install
.type keyboard_install, @function

.extern kmalloc
.type kmalloc, @function

.extern kmalloc_p
.type kmalloc_p, @function

.extern kmalloc_real
.type kmalloc_real, @function

.extern kpanic
.type kpanic, @function

.extern kvmalloc
.type kvmalloc, @function

.extern kvmalloc_p
.type kvmalloc_p, @function

.extern malloc
.type malloc, @function

.extern map_to_physical
.type map_to_physical, @function

.extern memcpy
.type memcpy, @function

.extern memort_total
.type memort_total, @function

.extern memory_use
.type memory_use, @function

.extern memset
.type memset, @function

.extern next_pid
.type next_pid, @function

.extern nframes
.type nframes, @function

.extern page_fault
.type page_fault, @function

.extern paging_finalize
.type paging_finalize, @function

.extern paging_install
.type paging_install, @function

.extern paging_mark_system
.type paging_mark_system, @function

.extern placement_pointer
.type placement_pointer, @function

.extern printf
.type printf, @function

.extern putchar
.type putchar, @function

.extern read_eip
.type read_eip, @function

.extern realloc
.type realloc, @function

.extern release_directory
.type release_directory, @function

.extern release_directory_for_exec
.type release_directory_for_exec, @function

.extern sbrk
.type sbrk, @function

.extern set_frame
.type set_frame, @function

.extern set_kernel_stack
.type set_kernel_stack, @function

.extern spin_init
.type spin_init, @function

.extern spin_lock
.type spin_lock, @function

.extern spin_unlock
.type spin_unlock, @function

.extern spin_wait
.type spin_wait, @function

.extern sprintf
.type sprintf, @function

.extern _start
.type _start, @function

.extern strcmp
.type strcmp, @function

.extern strcpy
.type strcpy, @function

.extern strlen
.type strlen, @function

.extern switch_page_directory
.type switch_page_directory, @function

.extern switch_task
.type switch_task, @function

.extern symbol_find
.type symbol_find, @function

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

.extern test_frame
.type test_frame, @function

.extern __TMC_END__
.type __TMC_END__, @function

.extern tss_flush
.type tss_flush, @function

.extern valloc
.type valloc, @function

.global kernel_symbols_start
kernel_symbols_start:

.long alloc_frame
.asciz "alloc_frame"

.long bss
.asciz "bss"

.long calloc
.asciz "calloc"

.long clear_frame
.asciz "clear_frame"

.long clone_directory
.asciz "clone_directory"

.long clone_table
.asciz "clone_table"

.long code
.asciz "code"

.long convert_scancode
.asciz "convert_scancode"

.long copy_page_physical
.asciz "copy_page_physical"

.long current_directory
.asciz "current_directory"

.long data
.asciz "data"

.long dma_frame
.asciz "dma_frame"

.long __dso_handle
.asciz "__dso_handle"

.long __DTOR_END__
.asciz "__DTOR_END__"

.long fault_handler
.asciz "fault_handler"

.long _fini
.asciz "_fini"

.long first_frame
.asciz "first_frame"

.long first_n_frames
.asciz "first_n_frames"

.long frames
.asciz "frames"

.long free
.asciz "free"

.long free_frame
.asciz "free_frame"

.long gdt_flush
.asciz "gdt_flush"

.long gdt_initialize
.asciz "gdt_initialize"

.long gdt_set_gate
.asciz "gdt_set_gate"

.long get_irq_handler
.asciz "get_irq_handler"

.long get_page
.asciz "get_page"

.long heap_end
.asciz "heap_end"

.long heap_install
.asciz "heap_install"

.long idt_initialize
.asciz "idt_initialize"

.long idt_load
.asciz "idt_load"

.long idt_set_gate
.asciz "idt_set_gate"

.long _init
.asciz "_init"

.long int_disable
.asciz "int_disable"

.long int_enable
.asciz "int_enable"

.long int_resume
.asciz "int_resume"

.long invalidate_page_tables
.asciz "invalidate_page_tables"

.long invalitate_tables_at
.asciz "invalitate_tables_at"

.long _irq0
.asciz "_irq0"

.long _irq1
.asciz "_irq1"

.long _irq10
.asciz "_irq10"

.long _irq11
.asciz "_irq11"

.long _irq12
.asciz "_irq12"

.long _irq13
.asciz "_irq13"

.long _irq14
.asciz "_irq14"

.long _irq15
.asciz "_irq15"

.long _irq2
.asciz "_irq2"

.long _irq3
.asciz "_irq3"

.long _irq4
.asciz "_irq4"

.long _irq5
.asciz "_irq5"

.long _irq6
.asciz "_irq6"

.long _irq7
.asciz "_irq7"

.long _irq8
.asciz "_irq8"

.long _irq9
.asciz "_irq9"

.long irq_ack
.asciz "irq_ack"

.long irq_handler
.asciz "irq_handler"

.long irq_initialize
.asciz "irq_initialize"

.long irq_install_handler
.asciz "irq_install_handler"

.long irq_uninstall_handler
.asciz "irq_uninstall_handler"

.long _isr0
.asciz "_isr0"

.long _isr1
.asciz "_isr1"

.long _isr10
.asciz "_isr10"

.long _isr11
.asciz "_isr11"

.long _isr12
.asciz "_isr12"

.long _isr127
.asciz "_isr127"

.long _isr13
.asciz "_isr13"

.long _isr14
.asciz "_isr14"

.long _isr15
.asciz "_isr15"

.long _isr16
.asciz "_isr16"

.long _isr17
.asciz "_isr17"

.long _isr18
.asciz "_isr18"

.long _isr19
.asciz "_isr19"

.long _isr2
.asciz "_isr2"

.long _isr20
.asciz "_isr20"

.long _isr21
.asciz "_isr21"

.long _isr22
.asciz "_isr22"

.long _isr23
.asciz "_isr23"

.long _isr24
.asciz "_isr24"

.long _isr25
.asciz "_isr25"

.long _isr26
.asciz "_isr26"

.long _isr27
.asciz "_isr27"

.long _isr28
.asciz "_isr28"

.long _isr29
.asciz "_isr29"

.long _isr3
.asciz "_isr3"

.long _isr30
.asciz "_isr30"

.long _isr31
.asciz "_isr31"

.long _isr4
.asciz "_isr4"

.long _isr5
.asciz "_isr5"

.long _isr6
.asciz "_isr6"

.long _isr7
.asciz "_isr7"

.long _isr8
.asciz "_isr8"

.long _isr9
.asciz "_isr9"

.long isr_initialize
.asciz "isr_initialize"

.long isr_install_handler
.asciz "isr_install_handler"

.long isr_uninstall_handler
.asciz "isr_uninstall_handler"

.long kernel_directory
.asciz "kernel_directory"

.long _kernel_end
.asciz "_kernel_end"

.long kernel_heap_alloc_point
.asciz "kernel_heap_alloc_point"

.long kernel_main
.asciz "kernel_main"

.long _kernel_start
.asciz "_kernel_start"

.long kernel_symbols_end
.asciz "kernel_symbols_end"

.long kernel_symbols_start
.asciz "kernel_symbols_start"

.long keyboard_install
.asciz "keyboard_install"

.long kmalloc
.asciz "kmalloc"

.long kmalloc_p
.asciz "kmalloc_p"

.long kmalloc_real
.asciz "kmalloc_real"

.long kpanic
.asciz "kpanic"

.long kvmalloc
.asciz "kvmalloc"

.long kvmalloc_p
.asciz "kvmalloc_p"

.long malloc
.asciz "malloc"

.long map_to_physical
.asciz "map_to_physical"

.long memcpy
.asciz "memcpy"

.long memort_total
.asciz "memort_total"

.long memory_use
.asciz "memory_use"

.long memset
.asciz "memset"

.long next_pid
.asciz "next_pid"

.long nframes
.asciz "nframes"

.long page_fault
.asciz "page_fault"

.long paging_finalize
.asciz "paging_finalize"

.long paging_install
.asciz "paging_install"

.long paging_mark_system
.asciz "paging_mark_system"

.long placement_pointer
.asciz "placement_pointer"

.long printf
.asciz "printf"

.long putchar
.asciz "putchar"

.long read_eip
.asciz "read_eip"

.long realloc
.asciz "realloc"

.long release_directory
.asciz "release_directory"

.long release_directory_for_exec
.asciz "release_directory_for_exec"

.long sbrk
.asciz "sbrk"

.long set_frame
.asciz "set_frame"

.long set_kernel_stack
.asciz "set_kernel_stack"

.long spin_init
.asciz "spin_init"

.long spin_lock
.asciz "spin_lock"

.long spin_unlock
.asciz "spin_unlock"

.long spin_wait
.asciz "spin_wait"

.long sprintf
.asciz "sprintf"

.long _start
.asciz "_start"

.long strcmp
.asciz "strcmp"

.long strcpy
.asciz "strcpy"

.long strlen
.asciz "strlen"

.long switch_page_directory
.asciz "switch_page_directory"

.long switch_task
.asciz "switch_task"

.long symbol_find
.asciz "symbol_find"

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

.long test_frame
.asciz "test_frame"

.long __TMC_END__
.asciz "__TMC_END__"

.long tss_flush
.asciz "tss_flush"

.long valloc
.asciz "valloc"

.global kernel_symbols_end
kernel_symbols_end:
