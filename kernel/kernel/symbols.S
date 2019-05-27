.section .symbols

.extern alloc_frame
.type alloc_frame, @function

.extern args_parse
.type args_parse, @function

.extern args_present
.type args_present, @function

.extern args_value
.type args_value, @function

.extern bitset_clear
.type bitset_clear, @function

.extern bitset_ffub
.type bitset_ffub, @function

.extern bitset_free
.type bitset_free, @function

.extern bitset_init
.type bitset_init, @function

.extern bitset_set
.type bitset_set, @function

.extern bitset_test
.type bitset_test, @function

.extern boot_time
.type boot_time, @function

.extern bss
.type bss, @function

.extern calloc
.type calloc, @function

.extern canonicalize_path
.type canonicalize_path, @function

.extern chmod_fs
.type chmod_fs, @function

.extern chown_fs
.type chown_fs, @function

.extern cleanup_process
.type cleanup_process, @function

.extern clear_frame
.type clear_frame, @function

.extern clone
.type clone, @function

.extern clone_directory
.type clone_directory, @function

.extern clone_fs
.type clone_fs, @function

.extern clone_table
.type clone_table, @function

.extern close_fs
.type close_fs, @function

.extern cmdline
.type cmdline, @function

.extern cmos_dump
.type cmos_dump, @function

.extern code
.type code, @function

.extern convert_scancode
.type convert_scancode, @function

.extern copy_page_physical
.type copy_page_physical, @function

.extern create_file_fs
.type create_file_fs, @function

.extern create_kernel_daemon
.type create_kernel_daemon, @function

.extern current_directory
.type current_directory, @function

.extern current_process
.type current_process, @function

.extern data
.type data, @function

.extern default_name
.type default_name, @function

.extern delete_process
.type delete_process, @function

.extern disable_fpu
.type disable_fpu, @function

.extern dma_frame
.type dma_frame, @function

.extern __dso_handle
.type __dso_handle, @function

.extern __DTOR_END__
.type __DTOR_END__, @function

.extern enable_fpu
.type enable_fpu, @function

.extern enter_signal_handler
.type enter_signal_handler, @function

.extern enter_user_jump
.type enter_user_jump, @function

.extern enter_userspace
.type enter_userspace, @function

.extern exec
.type exec, @function

.extern exec_elf
.type exec_elf, @function

.extern exec_shebang
.type exec_shebang, @function

.extern ext2_fs_mount
.type ext2_fs_mount, @function

.extern ext2_initialize
.type ext2_initialize, @function

.extern fault_handler
.type fault_handler, @function

.extern finddir_fs
.type finddir_fs, @function

.extern _fini
.type _fini, @function

.extern first_frame
.type first_frame, @function

.extern first_n_frames
.type first_n_frames, @function

.extern fix_signal_stacks
.type fix_signal_stacks, @function

.extern fmts
.type fmts, @function

.extern fork
.type fork, @function

.extern fpu_initialize
.type fpu_initialize, @function

.extern fpu_thread
.type fpu_thread, @function

.extern frames
.type frames, @function

.extern free
.type free, @function

.extern free_frame
.type free_frame, @function

.extern frozen_stack
.type frozen_stack, @function

.extern fs_root
.type fs_root, @function

.extern fs_tree
.type fs_tree, @function

.extern fs_types
.type fs_types, @function

.extern gdt_flush
.type gdt_flush, @function

.extern gdt_initialize
.type gdt_initialize, @function

.extern gdt_set_gate
.type gdt_set_gate, @function

.extern get_date
.type get_date, @function

.extern get_irq_handler
.type get_irq_handler, @function

.extern get_mount_point
.type get_mount_point, @function

.extern get_next_pid
.type get_next_pid, @function

.extern get_page
.type get_page, @function

.extern getpid
.type getpid, @function

.extern gettimeofday
.type gettimeofday, @function

.extern group_send_singal
.type group_send_singal, @function

.extern handle_signal
.type handle_signal, @function

.extern hashtable_create
.type hashtable_create, @function

.extern hashtable_create_int
.type hashtable_create_int, @function

.extern hashtable_free
.type hashtable_free, @function

.extern hashtable_get
.type hashtable_get, @function

.extern hashtable_has
.type hashtable_has, @function

.extern hashtable_int_comp
.type hashtable_int_comp, @function

.extern hashtable_int_dupe
.type hashtable_int_dupe, @function

.extern hashtable_int_hash
.type hashtable_int_hash, @function

.extern hashtable_is_empty
.type hashtable_is_empty, @function

.extern hashtable_keys
.type hashtable_keys, @function

.extern hashtable_remove
.type hashtable_remove, @function

.extern hashtable_set
.type hashtable_set, @function

.extern hashtable_string_comp
.type hashtable_string_comp, @function

.extern hashtable_string_dupe
.type hashtable_string_dupe, @function

.extern hashtable_string_hash
.type hashtable_string_hash, @function

.extern hashtable_values
.type hashtable_values, @function

.extern has_permission
.type has_permission, @function

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

.extern init_fpu
.type init_fpu, @function

.extern initial_esp
.type initial_esp, @function

.extern initialize_process_tree
.type initialize_process_tree, @function

.extern int_disable
.type int_disable, @function

.extern int_enable
.type int_enable, @function

.extern int_resume
.type int_resume, @function

.extern invalidate_page_tables
.type invalidate_page_tables, @function

.extern invalidate_tables_at
.type invalidate_tables_at, @function

.extern invalid_op
.type invalid_op, @function

.extern ioctl_fs
.type ioctl_fs, @function

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

.extern isdeadly
.type isdeadly, @function

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

.extern is_update_in_progress
.type is_update_in_progress, @function

.extern is_valid_process
.type is_valid_process, @function

.extern kernel_args_table
.type kernel_args_table, @function

.extern kernel_directory
.type kernel_directory, @function

.extern _kernel_end
.type _kernel_end, @function

.extern kernel_heap_alloc_point
.type kernel_heap_alloc_point, @function

.extern kernel_idle_task
.type kernel_idle_task, @function

.extern kernel_main
.type kernel_main, @function

.extern _kernel_start
.type _kernel_start, @function

.extern kernel_symbols_end
.type kernel_symbols_end, @function

.extern kernel_symbols_start
.type kernel_symbols_start, @function

.extern kexit
.type kexit, @function

.extern keyboard_install
.type keyboard_install, @function

.extern kmalloc
.type kmalloc, @function

.extern kmalloc_p
.type kmalloc_p, @function

.extern kmalloc_real
.type kmalloc_real, @function

.extern kmalloc_startat
.type kmalloc_startat, @function

.extern kopen
.type kopen, @function

.extern kopen_recur
.type kopen_recur, @function

.extern kpanic
.type kpanic, @function

.extern kvmalloc
.type kvmalloc, @function

.extern kvmalloc_p
.type kvmalloc_p, @function

.extern lfind
.type lfind, @function

.extern list_append
.type list_append, @function

.extern list_append_after
.type list_append_after, @function

.extern list_append_before
.type list_append_before, @function

.extern list_copy
.type list_copy, @function

.extern list_create
.type list_create, @function

.extern list_delete
.type list_delete, @function

.extern list_dequeue
.type list_dequeue, @function

.extern list_destroy
.type list_destroy, @function

.extern list_find
.type list_find, @function

.extern list_free
.type list_free, @function

.extern list_index_of
.type list_index_of, @function

.extern list_insert
.type list_insert, @function

.extern list_insert_after
.type list_insert_after, @function

.extern list_insert_before
.type list_insert_before, @function

.extern list_merge
.type list_merge, @function

.extern list_pop
.type list_pop, @function

.extern list_remove
.type list_remove, @function

.extern make_process_ready
.type make_process_ready, @function

.extern malloc
.type malloc, @function

.extern map_to_physical
.type map_to_physical, @function

.extern map_vfs_directory
.type map_vfs_directory, @function

.extern memchr
.type memchr, @function

.extern memcmp
.type memcmp, @function

.extern memcpy
.type memcpy, @function

.extern memmove
.type memmove, @function

.extern memort_total
.type memort_total, @function

.extern memory_use
.type memory_use, @function

.extern memset
.type memset, @function

.extern mkdir_fs
.type mkdir_fs, @function

.extern next_pid
.type next_pid, @function

.extern next_ready_process
.type next_ready_process, @function

.extern nframes
.type nframes, @function

.extern now
.type now, @function

.extern open_fs
.type open_fs, @function

.extern page_fault
.type page_fault, @function

.extern paging_finalize
.type paging_finalize, @function

.extern paging_initialize
.type paging_initialize, @function

.extern paging_mark_system
.type paging_mark_system, @function

.extern placement_pointer
.type placement_pointer, @function

.extern printf
.type printf, @function

.extern process_alert_node
.type process_alert_node, @function

.extern process_append_fd
.type process_append_fd, @function

.extern process_available
.type process_available, @function

.extern process_awaken_from_fswait
.type process_awaken_from_fswait, @function

.extern process_compare
.type process_compare, @function

.extern process_disown
.type process_disown, @function

.extern process_from_pid
.type process_from_pid, @function

.extern process_get_parent
.type process_get_parent, @function

.extern process_is_ready
.type process_is_ready, @function

.extern process_list
.type process_list, @function

.extern process_move_fd
.type process_move_fd, @function

.extern process_queue
.type process_queue, @function

.extern process_tree
.type process_tree, @function

.extern process_wait_nodes
.type process_wait_nodes, @function

.extern putchar
.type putchar, @function

.extern ramdisk_mount
.type ramdisk_mount, @function

.extern read_cmos
.type read_cmos, @function

.extern readdir_fs
.type readdir_fs, @function

.extern read_eip
.type read_eip, @function

.extern read_fs
.type read_fs, @function

.extern readlink_fs
.type readlink_fs, @function

.extern realloc
.type realloc, @function

.extern reap_process
.type reap_process, @function

.extern relative_time
.type relative_time, @function

.extern release_directory
.type release_directory, @function

.extern release_directory_for_exec
.type release_directory_for_exec, @function

.extern restore_fpu
.type restore_fpu, @function

.extern rets_from_sig
.type rets_from_sig, @function

.extern return_from_signal_handler
.type return_from_signal_handler, @function

.extern return_to_userspace
.type return_to_userspace, @function

.extern save_fpu
.type save_fpu, @function

.extern saves
.type saves, @function

.extern sbrk
.type sbrk, @function

.extern secs_of_month
.type secs_of_month, @function

.extern secs_of_years
.type secs_of_years, @function

.extern selectcheck_fs
.type selectcheck_fs, @function

.extern selectwait_fs
.type selectwait_fs, @function

.extern send_signal
.type send_signal, @function

.extern set_fpu_cw
.type set_fpu_cw, @function

.extern set_frame
.type set_frame, @function

.extern set_kernel_stack
.type set_kernel_stack, @function

.extern set_process_environment
.type set_process_environment, @function

.extern shm_initialize
.type shm_initialize, @function

.extern shm_obtain
.type shm_obtain, @function

.extern shm_release
.type shm_release, @function

.extern shm_release_all
.type shm_release_all, @function

.extern shm_tree
.type shm_tree, @function

.extern sleep_on
.type sleep_on, @function

.extern sleep_queue
.type sleep_queue, @function

.extern sleep_until
.type sleep_until, @function

.extern spawn_init
.type spawn_init, @function

.extern spawn_kidle
.type spawn_kidle, @function

.extern spawn_process
.type spawn_process, @function

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

.extern strcat
.type strcat, @function

.extern strchr
.type strchr, @function

.extern strchrnul
.type strchrnul, @function

.extern strcmp
.type strcmp, @function

.extern strcpy
.type strcpy, @function

.extern strcspn
.type strcspn, @function

.extern strdup
.type strdup, @function

.extern strlen
.type strlen, @function

.extern strncat
.type strncat, @function

.extern strpbrk
.type strpbrk, @function

.extern strspn
.type strspn, @function

.extern strstr
.type strstr, @function

.extern strtok
.type strtok, @function

.extern strtok_r
.type strtok_r, @function

.extern switch_fpu
.type switch_fpu, @function

.extern switch_next
.type switch_next, @function

.extern switch_page_directory
.type switch_page_directory, @function

.extern switch_task
.type switch_task, @function

.extern symbol_find
.type symbol_find, @function

.extern symlink_fs
.type symlink_fs, @function

.extern system
.type system, @function

.extern tarfs_initialize
.type tarfs_initialize, @function

.extern tar_mount
.type tar_mount, @function

.extern task_exit
.type task_exit, @function

.extern tasking_initialize
.type tasking_initialize, @function

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

.extern _timer_drift
.type _timer_drift, @function

.extern timer_drift
.type timer_drift, @function

.extern timer_handler
.type timer_handler, @function

.extern timer_initialize
.type timer_initialize, @function

.extern timer_phase
.type timer_phase, @function

.extern timer_subticks
.type timer_subticks, @function

.extern timer_ticks
.type timer_ticks, @function

.extern __TMC_END__
.type __TMC_END__, @function

.extern tree_break_off
.type tree_break_off, @function

.extern tree_count_children
.type tree_count_children, @function

.extern tree_create
.type tree_create, @function

.extern tree_destroy
.type tree_destroy, @function

.extern tree_find
.type tree_find, @function

.extern tree_find_parent
.type tree_find_parent, @function

.extern tree_free
.type tree_free, @function

.extern tree_node_create
.type tree_node_create, @function

.extern tree_node_destroy
.type tree_node_destroy, @function

.extern tree_node_find
.type tree_node_find, @function

.extern tree_node_find_parent
.type tree_node_find_parent, @function

.extern tree_node_free
.type tree_node_free, @function

.extern tree_node_insert_child
.type tree_node_insert_child, @function

.extern tree_node_insert_child_node
.type tree_node_insert_child_node, @function

.extern tree_node_parent_remove
.type tree_node_parent_remove, @function

.extern tree_node_remove
.type tree_node_remove, @function

.extern tree_remove
.type tree_remove, @function

.extern tree_remove_reparent_root
.type tree_remove_reparent_root, @function

.extern tree_set_root
.type tree_set_root, @function

.extern truncate_fs
.type truncate_fs, @function

.extern tss_flush
.type tss_flush, @function

.extern unlink_fs
.type unlink_fs, @function

.extern unswitch_fpu
.type unswitch_fpu, @function

.extern valloc
.type valloc, @function

.extern vfs_initialize
.type vfs_initialize, @function

.extern vfs_lock
.type vfs_lock, @function

.extern vfs_mount
.type vfs_mount, @function

.extern vfs_mount_type
.type vfs_mount_type, @function

.extern vfs_register
.type vfs_register, @function

.extern waitpid
.type waitpid, @function

.extern wakeup_queue
.type wakeup_queue, @function

.extern wakeup_queue_interrupted
.type wakeup_queue_interrupted, @function

.extern wakeup_sleepers
.type wakeup_sleepers, @function

.extern write_fs
.type write_fs, @function

.global kernel_symbols_start
kernel_symbols_start:

.long alloc_frame
.asciz "alloc_frame"

.long args_parse
.asciz "args_parse"

.long args_present
.asciz "args_present"

.long args_value
.asciz "args_value"

.long bitset_clear
.asciz "bitset_clear"

.long bitset_ffub
.asciz "bitset_ffub"

.long bitset_free
.asciz "bitset_free"

.long bitset_init
.asciz "bitset_init"

.long bitset_set
.asciz "bitset_set"

.long bitset_test
.asciz "bitset_test"

.long boot_time
.asciz "boot_time"

.long bss
.asciz "bss"

.long calloc
.asciz "calloc"

.long canonicalize_path
.asciz "canonicalize_path"

.long chmod_fs
.asciz "chmod_fs"

.long chown_fs
.asciz "chown_fs"

.long cleanup_process
.asciz "cleanup_process"

.long clear_frame
.asciz "clear_frame"

.long clone
.asciz "clone"

.long clone_directory
.asciz "clone_directory"

.long clone_fs
.asciz "clone_fs"

.long clone_table
.asciz "clone_table"

.long close_fs
.asciz "close_fs"

.long cmdline
.asciz "cmdline"

.long cmos_dump
.asciz "cmos_dump"

.long code
.asciz "code"

.long convert_scancode
.asciz "convert_scancode"

.long copy_page_physical
.asciz "copy_page_physical"

.long create_file_fs
.asciz "create_file_fs"

.long create_kernel_daemon
.asciz "create_kernel_daemon"

.long current_directory
.asciz "current_directory"

.long current_process
.asciz "current_process"

.long data
.asciz "data"

.long default_name
.asciz "default_name"

.long delete_process
.asciz "delete_process"

.long disable_fpu
.asciz "disable_fpu"

.long dma_frame
.asciz "dma_frame"

.long __dso_handle
.asciz "__dso_handle"

.long __DTOR_END__
.asciz "__DTOR_END__"

.long enable_fpu
.asciz "enable_fpu"

.long enter_signal_handler
.asciz "enter_signal_handler"

.long enter_user_jump
.asciz "enter_user_jump"

.long enter_userspace
.asciz "enter_userspace"

.long exec
.asciz "exec"

.long exec_elf
.asciz "exec_elf"

.long exec_shebang
.asciz "exec_shebang"

.long ext2_fs_mount
.asciz "ext2_fs_mount"

.long ext2_initialize
.asciz "ext2_initialize"

.long fault_handler
.asciz "fault_handler"

.long finddir_fs
.asciz "finddir_fs"

.long _fini
.asciz "_fini"

.long first_frame
.asciz "first_frame"

.long first_n_frames
.asciz "first_n_frames"

.long fix_signal_stacks
.asciz "fix_signal_stacks"

.long fmts
.asciz "fmts"

.long fork
.asciz "fork"

.long fpu_initialize
.asciz "fpu_initialize"

.long fpu_thread
.asciz "fpu_thread"

.long frames
.asciz "frames"

.long free
.asciz "free"

.long free_frame
.asciz "free_frame"

.long frozen_stack
.asciz "frozen_stack"

.long fs_root
.asciz "fs_root"

.long fs_tree
.asciz "fs_tree"

.long fs_types
.asciz "fs_types"

.long gdt_flush
.asciz "gdt_flush"

.long gdt_initialize
.asciz "gdt_initialize"

.long gdt_set_gate
.asciz "gdt_set_gate"

.long get_date
.asciz "get_date"

.long get_irq_handler
.asciz "get_irq_handler"

.long get_mount_point
.asciz "get_mount_point"

.long get_next_pid
.asciz "get_next_pid"

.long get_page
.asciz "get_page"

.long getpid
.asciz "getpid"

.long gettimeofday
.asciz "gettimeofday"

.long group_send_singal
.asciz "group_send_singal"

.long handle_signal
.asciz "handle_signal"

.long hashtable_create
.asciz "hashtable_create"

.long hashtable_create_int
.asciz "hashtable_create_int"

.long hashtable_free
.asciz "hashtable_free"

.long hashtable_get
.asciz "hashtable_get"

.long hashtable_has
.asciz "hashtable_has"

.long hashtable_int_comp
.asciz "hashtable_int_comp"

.long hashtable_int_dupe
.asciz "hashtable_int_dupe"

.long hashtable_int_hash
.asciz "hashtable_int_hash"

.long hashtable_is_empty
.asciz "hashtable_is_empty"

.long hashtable_keys
.asciz "hashtable_keys"

.long hashtable_remove
.asciz "hashtable_remove"

.long hashtable_set
.asciz "hashtable_set"

.long hashtable_string_comp
.asciz "hashtable_string_comp"

.long hashtable_string_dupe
.asciz "hashtable_string_dupe"

.long hashtable_string_hash
.asciz "hashtable_string_hash"

.long hashtable_values
.asciz "hashtable_values"

.long has_permission
.asciz "has_permission"

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

.long init_fpu
.asciz "init_fpu"

.long initial_esp
.asciz "initial_esp"

.long initialize_process_tree
.asciz "initialize_process_tree"

.long int_disable
.asciz "int_disable"

.long int_enable
.asciz "int_enable"

.long int_resume
.asciz "int_resume"

.long invalidate_page_tables
.asciz "invalidate_page_tables"

.long invalidate_tables_at
.asciz "invalidate_tables_at"

.long invalid_op
.asciz "invalid_op"

.long ioctl_fs
.asciz "ioctl_fs"

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

.long isdeadly
.asciz "isdeadly"

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

.long is_update_in_progress
.asciz "is_update_in_progress"

.long is_valid_process
.asciz "is_valid_process"

.long kernel_args_table
.asciz "kernel_args_table"

.long kernel_directory
.asciz "kernel_directory"

.long _kernel_end
.asciz "_kernel_end"

.long kernel_heap_alloc_point
.asciz "kernel_heap_alloc_point"

.long kernel_idle_task
.asciz "kernel_idle_task"

.long kernel_main
.asciz "kernel_main"

.long _kernel_start
.asciz "_kernel_start"

.long kernel_symbols_end
.asciz "kernel_symbols_end"

.long kernel_symbols_start
.asciz "kernel_symbols_start"

.long kexit
.asciz "kexit"

.long keyboard_install
.asciz "keyboard_install"

.long kmalloc
.asciz "kmalloc"

.long kmalloc_p
.asciz "kmalloc_p"

.long kmalloc_real
.asciz "kmalloc_real"

.long kmalloc_startat
.asciz "kmalloc_startat"

.long kopen
.asciz "kopen"

.long kopen_recur
.asciz "kopen_recur"

.long kpanic
.asciz "kpanic"

.long kvmalloc
.asciz "kvmalloc"

.long kvmalloc_p
.asciz "kvmalloc_p"

.long lfind
.asciz "lfind"

.long list_append
.asciz "list_append"

.long list_append_after
.asciz "list_append_after"

.long list_append_before
.asciz "list_append_before"

.long list_copy
.asciz "list_copy"

.long list_create
.asciz "list_create"

.long list_delete
.asciz "list_delete"

.long list_dequeue
.asciz "list_dequeue"

.long list_destroy
.asciz "list_destroy"

.long list_find
.asciz "list_find"

.long list_free
.asciz "list_free"

.long list_index_of
.asciz "list_index_of"

.long list_insert
.asciz "list_insert"

.long list_insert_after
.asciz "list_insert_after"

.long list_insert_before
.asciz "list_insert_before"

.long list_merge
.asciz "list_merge"

.long list_pop
.asciz "list_pop"

.long list_remove
.asciz "list_remove"

.long make_process_ready
.asciz "make_process_ready"

.long malloc
.asciz "malloc"

.long map_to_physical
.asciz "map_to_physical"

.long map_vfs_directory
.asciz "map_vfs_directory"

.long memchr
.asciz "memchr"

.long memcmp
.asciz "memcmp"

.long memcpy
.asciz "memcpy"

.long memmove
.asciz "memmove"

.long memort_total
.asciz "memort_total"

.long memory_use
.asciz "memory_use"

.long memset
.asciz "memset"

.long mkdir_fs
.asciz "mkdir_fs"

.long next_pid
.asciz "next_pid"

.long next_ready_process
.asciz "next_ready_process"

.long nframes
.asciz "nframes"

.long now
.asciz "now"

.long open_fs
.asciz "open_fs"

.long page_fault
.asciz "page_fault"

.long paging_finalize
.asciz "paging_finalize"

.long paging_initialize
.asciz "paging_initialize"

.long paging_mark_system
.asciz "paging_mark_system"

.long placement_pointer
.asciz "placement_pointer"

.long printf
.asciz "printf"

.long process_alert_node
.asciz "process_alert_node"

.long process_append_fd
.asciz "process_append_fd"

.long process_available
.asciz "process_available"

.long process_awaken_from_fswait
.asciz "process_awaken_from_fswait"

.long process_compare
.asciz "process_compare"

.long process_disown
.asciz "process_disown"

.long process_from_pid
.asciz "process_from_pid"

.long process_get_parent
.asciz "process_get_parent"

.long process_is_ready
.asciz "process_is_ready"

.long process_list
.asciz "process_list"

.long process_move_fd
.asciz "process_move_fd"

.long process_queue
.asciz "process_queue"

.long process_tree
.asciz "process_tree"

.long process_wait_nodes
.asciz "process_wait_nodes"

.long putchar
.asciz "putchar"

.long ramdisk_mount
.asciz "ramdisk_mount"

.long read_cmos
.asciz "read_cmos"

.long readdir_fs
.asciz "readdir_fs"

.long read_eip
.asciz "read_eip"

.long read_fs
.asciz "read_fs"

.long readlink_fs
.asciz "readlink_fs"

.long realloc
.asciz "realloc"

.long reap_process
.asciz "reap_process"

.long relative_time
.asciz "relative_time"

.long release_directory
.asciz "release_directory"

.long release_directory_for_exec
.asciz "release_directory_for_exec"

.long restore_fpu
.asciz "restore_fpu"

.long rets_from_sig
.asciz "rets_from_sig"

.long return_from_signal_handler
.asciz "return_from_signal_handler"

.long return_to_userspace
.asciz "return_to_userspace"

.long save_fpu
.asciz "save_fpu"

.long saves
.asciz "saves"

.long sbrk
.asciz "sbrk"

.long secs_of_month
.asciz "secs_of_month"

.long secs_of_years
.asciz "secs_of_years"

.long selectcheck_fs
.asciz "selectcheck_fs"

.long selectwait_fs
.asciz "selectwait_fs"

.long send_signal
.asciz "send_signal"

.long set_fpu_cw
.asciz "set_fpu_cw"

.long set_frame
.asciz "set_frame"

.long set_kernel_stack
.asciz "set_kernel_stack"

.long set_process_environment
.asciz "set_process_environment"

.long shm_initialize
.asciz "shm_initialize"

.long shm_obtain
.asciz "shm_obtain"

.long shm_release
.asciz "shm_release"

.long shm_release_all
.asciz "shm_release_all"

.long shm_tree
.asciz "shm_tree"

.long sleep_on
.asciz "sleep_on"

.long sleep_queue
.asciz "sleep_queue"

.long sleep_until
.asciz "sleep_until"

.long spawn_init
.asciz "spawn_init"

.long spawn_kidle
.asciz "spawn_kidle"

.long spawn_process
.asciz "spawn_process"

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

.long strcat
.asciz "strcat"

.long strchr
.asciz "strchr"

.long strchrnul
.asciz "strchrnul"

.long strcmp
.asciz "strcmp"

.long strcpy
.asciz "strcpy"

.long strcspn
.asciz "strcspn"

.long strdup
.asciz "strdup"

.long strlen
.asciz "strlen"

.long strncat
.asciz "strncat"

.long strpbrk
.asciz "strpbrk"

.long strspn
.asciz "strspn"

.long strstr
.asciz "strstr"

.long strtok
.asciz "strtok"

.long strtok_r
.asciz "strtok_r"

.long switch_fpu
.asciz "switch_fpu"

.long switch_next
.asciz "switch_next"

.long switch_page_directory
.asciz "switch_page_directory"

.long switch_task
.asciz "switch_task"

.long symbol_find
.asciz "symbol_find"

.long symlink_fs
.asciz "symlink_fs"

.long system
.asciz "system"

.long tarfs_initialize
.asciz "tarfs_initialize"

.long tar_mount
.asciz "tar_mount"

.long task_exit
.asciz "task_exit"

.long tasking_initialize
.asciz "tasking_initialize"

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

.long _timer_drift
.asciz "_timer_drift"

.long timer_drift
.asciz "timer_drift"

.long timer_handler
.asciz "timer_handler"

.long timer_initialize
.asciz "timer_initialize"

.long timer_phase
.asciz "timer_phase"

.long timer_subticks
.asciz "timer_subticks"

.long timer_ticks
.asciz "timer_ticks"

.long __TMC_END__
.asciz "__TMC_END__"

.long tree_break_off
.asciz "tree_break_off"

.long tree_count_children
.asciz "tree_count_children"

.long tree_create
.asciz "tree_create"

.long tree_destroy
.asciz "tree_destroy"

.long tree_find
.asciz "tree_find"

.long tree_find_parent
.asciz "tree_find_parent"

.long tree_free
.asciz "tree_free"

.long tree_node_create
.asciz "tree_node_create"

.long tree_node_destroy
.asciz "tree_node_destroy"

.long tree_node_find
.asciz "tree_node_find"

.long tree_node_find_parent
.asciz "tree_node_find_parent"

.long tree_node_free
.asciz "tree_node_free"

.long tree_node_insert_child
.asciz "tree_node_insert_child"

.long tree_node_insert_child_node
.asciz "tree_node_insert_child_node"

.long tree_node_parent_remove
.asciz "tree_node_parent_remove"

.long tree_node_remove
.asciz "tree_node_remove"

.long tree_remove
.asciz "tree_remove"

.long tree_remove_reparent_root
.asciz "tree_remove_reparent_root"

.long tree_set_root
.asciz "tree_set_root"

.long truncate_fs
.asciz "truncate_fs"

.long tss_flush
.asciz "tss_flush"

.long unlink_fs
.asciz "unlink_fs"

.long unswitch_fpu
.asciz "unswitch_fpu"

.long valloc
.asciz "valloc"

.long vfs_initialize
.asciz "vfs_initialize"

.long vfs_lock
.asciz "vfs_lock"

.long vfs_mount
.asciz "vfs_mount"

.long vfs_mount_type
.asciz "vfs_mount_type"

.long vfs_register
.asciz "vfs_register"

.long waitpid
.asciz "waitpid"

.long wakeup_queue
.asciz "wakeup_queue"

.long wakeup_queue_interrupted
.asciz "wakeup_queue_interrupted"

.long wakeup_sleepers
.asciz "wakeup_sleepers"

.long write_fs
.asciz "write_fs"

.global kernel_symbols_end
kernel_symbols_end:
