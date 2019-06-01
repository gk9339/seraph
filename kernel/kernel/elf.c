#include <kernel/elf.h>
#include <stddef.h>
#include <string.h>
#include <stdlib.h>
#include <kernel/process.h>
#include <kernel/task.h>
#include <errno.h>
#include <kernel/irq.h>

typedef int (*exec_func)(char* path, fs_node_t* file, int argc, char** argv, char** env, int interp);

typedef struct 
{
	exec_func func;
	unsigned char bytes[4];
	unsigned int  match;
	char* name;
}exec_def_t;

static int exec_elf( char* path __attribute__((unused)), fs_node_t* file, int argc, char** argv, char** env, int interp __attribute__((unused)) )
{
	Elf32_Header header;

	read_fs(file, 0, sizeof(Elf32_Header), (uint8_t *)&header);

	if( header.e_ident[0] != ELFMAG0 ||
	    header.e_ident[1] != ELFMAG1 ||
	    header.e_ident[2] != ELFMAG2 ||
	    header.e_ident[3] != ELFMAG3 )
    {
		close_fs(file);
		return -1;
	}

	if( file->mask & 0x800 )
    {
		current_process->user = file->uid;
	}

	for( uintptr_t x = 0; x < (uint32_t)header.e_phentsize * header.e_phnum; x += header.e_phentsize )
    {
		Elf32_Phdr phdr;
		read_fs(file, header.e_phoff + x, sizeof(Elf32_Phdr), (uint8_t *)&phdr);
		if( phdr.p_type == PT_DYNAMIC )
        {
			/* Dynamic */
			close_fs(file);

			unsigned int nargc = argc + 3;
			char* args[nargc+1];
			args[0] = "ld.so";
			args[1] = "-e";
			args[2] = strdup(current_process->name);
			int j = 3;
			for( int i = 0; i < argc; i++, j++ )
            {
				args[j] = argv[i];
			}
			args[j] = NULL;

			fs_node_t* ldfile = kopen("/lib/ld.so",0);
			if( !ldfile ) return -1;

			return exec_elf(NULL, ldfile, nargc, args, env, 1);
		}
	}

	uintptr_t entry = (uintptr_t)header.e_entry;
	uintptr_t base_addr = 0xFFFFFFFF;
	uintptr_t end_addr  = 0x0;

	for( uintptr_t x = 0; x < (uint32_t)header.e_phentsize * header.e_phnum; x += header.e_phentsize )
    {
		Elf32_Phdr phdr;
		read_fs(file, header.e_phoff + x, sizeof(Elf32_Phdr), (uint8_t*)&phdr);
		if( phdr.p_type == PT_LOAD )
        {
			if( phdr.p_vaddr < base_addr ) 
            {
				base_addr = phdr.p_vaddr;
			}
			if( phdr.p_memsz + phdr.p_vaddr > end_addr )
            {
				end_addr = phdr.p_memsz + phdr.p_vaddr;
			}
		}
	}

	current_process->image.entry = base_addr;
	current_process->image.size  = end_addr - base_addr;

	release_directory_for_exec(current_directory);
	invalidate_page_tables();


	for( uintptr_t x = 0; x < (uint32_t)header.e_phentsize * header.e_phnum; x += header.e_phentsize )
    { 
		Elf32_Phdr phdr;
		read_fs(file, header.e_phoff + x, sizeof(Elf32_Phdr), (uint8_t*)&phdr);
		if( phdr.p_type == PT_LOAD )
        {
			/* TODO: These virtual address bounds should be in a header somewhere */
			if (phdr.p_vaddr < 0x20000000) return -EINVAL;
			/* TODO Upper bounds */
			for( uintptr_t i = phdr.p_vaddr; i < phdr.p_vaddr + phdr.p_memsz; i += 0x1000 )
            {
				/* This doesn't care if we already allocated this page */
				alloc_frame(get_page(i, 1, current_directory), 0, 1);
				invalidate_tables_at(i);
			}
			int_resume();
			read_fs(file, phdr.p_offset, phdr.p_filesz, (uint8_t *)phdr.p_vaddr);
			int_disable();
			size_t r = phdr.p_filesz;
			while( r < phdr.p_memsz )
            {
				*(char*)(phdr.p_vaddr + r) = 0;
				r++;
			}
		}
	}

	close_fs(file);

	for( uintptr_t stack_pointer = USER_STACK_BOTTOM; stack_pointer < USER_STACK_TOP; stack_pointer += 0x1000 )
    {
		alloc_frame(get_page(stack_pointer, 1, current_directory), 0, 1);
		invalidate_tables_at(stack_pointer);
	}

	/* Collect arguments */
	int envc = 0;
	for( envc = 0; env[envc] != NULL; envc++ );

	/* Format auxv */
	Elf32_auxv auxv[] = 
    {
		{256, 0xDEADBEEF},
		{0, 0}
	};
	int auxvc = 0;
	for( auxvc = 0; auxv[auxvc].id != 0; auxvc++ );
	auxvc++;

	uintptr_t heap = current_process->image.entry + current_process->image.size;
	while( heap & 0xFFF ) heap++;
	alloc_frame(get_page(heap, 1, current_directory), 0, 1);
	invalidate_tables_at(heap);
	char** argv_ = (char**)heap;
	heap += sizeof(char*) * (argc + 1);
	char** env_ = (char**)heap;
	heap += sizeof(char*) * (envc + 1);
	void* auxv_ptr = (void*)heap;
	heap += sizeof(Elf32_auxv) * (auxvc);

	for( int i = 0; i < argc; i++ )
    {
		size_t size = strlen(argv[i]) * sizeof(char) + 1;
		for( uintptr_t x = heap; x < heap + size + 0x1000; x += 0x1000 )
        {
			alloc_frame(get_page(x, 1, current_directory), 0, 1);
		}
		invalidate_tables_at(heap);
		argv_[i] = (char*)heap;
		memcpy((void*)heap, argv[i], size);
		heap += size;
	}
	/* Don't forget the NULL at the end of that... */
	argv_[argc] = 0;

	for( int i = 0; i < envc; i++ )
    {
		size_t size = strlen(env[i]) * sizeof(char) + 1;
		for( uintptr_t x = heap; x < heap + size + 0x1000; x += 0x1000 )
        {
			alloc_frame(get_page(x, 1, current_directory), 0, 1);
		}
		invalidate_tables_at(heap);
		env_[i] = (char*)heap;
		memcpy((void*)heap, env[i], size);
		heap += size;
	}
	env_[envc] = 0;

	memcpy(auxv_ptr, auxv, sizeof(Elf32_auxv) * (auxvc));

	current_process->image.heap        = heap; /* heap end */
	current_process->image.heap_actual = heap + (0x1000 - heap % 0x1000);
	alloc_frame(get_page(current_process->image.heap_actual, 1, current_directory), 0, 1);
	invalidate_tables_at(current_process->image.heap_actual);
	current_process->image.user_stack  = USER_STACK_TOP;

	current_process->image.start = entry;

	/* Close all fds >= 3 */
	for( unsigned int i = 3; i < current_process->fds->length; i++ )
    {
		if( current_process->fds->entries[i] )
        {
			close_fs(current_process->fds->entries[i]);
			current_process->fds->entries[i] = NULL;
		}
	}

	/* Go go go */
	enter_user_jump(entry, argc, argv_, USER_STACK_TOP);

	/* We should never reach this code */
	return -1;
}

static int exec_shebang( char* path, fs_node_t* file, int argc, char** argv, char** env, int interp __attribute__((unused)) )
{
	/* Read MAX_LINE... */
	char tmp[100];
	read_fs(file, 0, 100, (unsigned char *)tmp); close_fs(file);
	char* cmd = (char *)&tmp[2];
	if( *cmd == ' ' ) cmd++; /* Handle a leading space */
	char* space_or_linefeed = strpbrk(cmd, " \n");
	char* arg = NULL;

	if( !space_or_linefeed )
    {
		return -ENOEXEC;
	}

	if( *space_or_linefeed == ' ' )
    {
		/* Oh lovely, an argument */
		*space_or_linefeed = '\0';
		space_or_linefeed++;
		arg = space_or_linefeed;
		space_or_linefeed = strpbrk(space_or_linefeed, "\n");
		if( !space_or_linefeed )
        {
			return -ENOEXEC;
		}
	}
	*space_or_linefeed = '\0';

	char script[strlen(path)+1];
	memcpy(script, path, strlen(path)+1);

	unsigned int nargc = argc + (arg ? 2 : 1);
	char* args[nargc + 2];
	args[0] = cmd;
	args[1] = arg ? arg : script;
	args[2] = arg ? script : NULL;
	args[3] = NULL;

	int j = arg ? 3 : 2;
	for( int i = 1; i < argc; i++, j++ )
    {
		args[j] = argv[i];
	}
	args[j] = NULL;

	return exec(cmd, nargc, args, env);
}

exec_def_t fmts[] = 
{
	{exec_elf, {ELFMAG0, ELFMAG1, ELFMAG2, ELFMAG3}, 4, "ELF"},
	{exec_shebang, {'#', '!', 0, 0}, 2, "#!"},
};

static int matches( unsigned char* a, unsigned char* b, unsigned int len )
{
    for( unsigned int i = 0; i < len; i++ )
    {
        if(a[i] != b[i]) return 0;
    }
    
    return 1;
}

int system( char* path, int argc, char** argv, char** envin )
{
    char** argv_ = malloc((argc+1) * sizeof(char*));

    for( int i = 0; i < argc; i++ )
    {
        argv_[i] = malloc((strlen(argv[i]) + 1) * sizeof(char));
        memcpy(argv_[i], argv[i], strlen(argv[i]) + 1);
    }

    argv_[argc] = 0;
    char* env[] = { NULL };
    set_process_environment((process_t*)current_process, clone_directory(current_directory));
    current_directory = current_process->thread.page_directory;
    switch_page_directory(current_directory);

    current_process->cmdline = argv_;

    exec(path, argc, argv_, envin? envin:env);
    kexit(-1);

    return -1;
}

int exec( char* path, int argc, char** argv, char** env )
{
    fs_node_t* file = kopen(path, 0);
    if( !file )
    {
        return -ENOENT;
    }

    if( !has_permission(file, 01) )
    {
        return -EACCES;
    }

    unsigned char head[4];
    read_fs(file, 0, 4, head);

    current_process->name = strdup(path);
    gettimeofday((struct timeval*)&current_process->start, NULL);

    for( unsigned int i = 0; i < sizeof(fmts) / sizeof(exec_def_t); i++ )
    {
        if(matches(fmts[i].bytes, head, fmts[i].match))
        {
            return fmts[i].func(path, file, argc, argv, env, 0);
        }
    }

    return -ENOEXEC;
}
