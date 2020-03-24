#include <sys/types.h>
#include <list.h>
#include <hashtable.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <unistd.h>
#include <sys/stat.h>
#include <sys/mman.h>
#include <kernel/elf.h>

typedef int (*entry_point_t)(int, char*[], char**);

typedef struct elf_object
{
    FILE* file;

    Elf32_Header header;

    char* dyn_string_table;
    size_t dyn_string_table_size;

    Elf32_Sym* dyn_symbol_table;
    size_t dyn_symbol_table_size;

    Elf32_Dyn* dynamic;
    Elf32_Word* dyn_hash;

    void (*init)(void);
    void (**init_array)(void);
    size_t init_array_size;

    uintptr_t base;

    list_t* dependencies;

    int loaded;
} elf_t;

typedef struct
{
    char* name;
    void* symbol;
} ld_exports_t;

static hashtable_t* symbol_table;
static hashtable_t* glob_dat;
static hashtable_t* objects_table;

static int _target_is_suid = 0;
uintptr_t _malloc_minimum = 0;

void* (*_malloc)(size_t size) = malloc;
void (*_free)(void* ptr) = free;

static elf_t* _main_obj = NULL;

//Locate library for LD_LIBRARY_PATH
static char* find_lib( const char* file );

//Open an object file
static elf_t* open_object( const char* path );

//Calculate the size of an object file by examining it's phdrs
static size_t object_calculate_size( elf_t* object );

//Load an object into memory
static uintptr_t object_load( elf_t* object, uintptr_t base );

//Perform cleanup after loading
static int object_postload( elf_t* object );

//Are symbol address needed for relocation type
static int need_symbol_for_type( unsigned char type );

//Apply ELF relocations
static int object_relocate( elf_t* object );

//Copy relocations need to be located before other relocations
static void object_find_copy_relocations( elf_t* object );

//Find a symbol in a specific object
static void* object_find_symbol( elf_t* object, const char* symbol_name );

//Fully load an object
static void* do_actual_load( const char* filename, elf_t* lib, int flags );

static void* dlopen_ld( const char* filename, int flags );
static int dlclose_ld( elf_t* lib );
static char* dlerror_ld( void );

//Used by libc
static void* _argv_value = NULL;
static char* argv_value(void)
{
    return _argv_value;
}

ld_exports_t ld_builtin_exports[] =
{
    {"dlopen", dlopen_ld},
    {"dlsym", object_find_symbol},
    {"dlclose", dlclose_ld},
    {"dlerror", dlerror_ld},
    {"__get_argv", argv_value},
    {NULL, NULL},
};

#undef malloc
#undef free
#define malloc ld_malloc
#define free ld_free
static void* malloc( size_t size )
{
    return _malloc(size);
}

static void free( void* ptr )
{
    if( (uintptr_t)ptr > _malloc_minimum )
    {
        _free(ptr);
    }
}

int main( int argc, char** argv )
{
    char* file = argv[1];
    size_t argc_offset = 1;

    if( !strcmp(argv[1], "-e") )
    {
        argc_offset = 3;
        file = argv[2];
    }

    _argv_value = argv+argc_offset;

    symbol_table = hashtable_create(10);
    glob_dat = hashtable_create(10);
    objects_table = hashtable_create(10);

    ld_exports_t* exp = ld_builtin_exports;
    while( exp->name )
    {
        hashtable_set(symbol_table, exp->name, exp->symbol);
        exp++;
    }

    struct stat buf;
    if( stat(file, &buf) )
    {
        //TODO stderr
        printf("%s: target binary '%s' not available\n", argv[0], file);
    }

    if( buf.st_mode & S_ISUID )
    {
        _target_is_suid = 1;
    }

    elf_t* main_obj = open_object(file);
    _main_obj = main_obj;

    if( !main_obj )
    {
        //TODO stderr
        return 1;
    }

    // Load the main object
    uintptr_t end_addr = object_load(main_obj, 0x0);
    object_postload(main_obj);
    object_find_copy_relocations(main_obj);

    // Load library dependencies
    hashtable_t* libs = hashtable_create(10);

    while( end_addr & 0xFFF )
    {
        end_addr++;
    }

    list_t* ctor_libs = list_create();
    list_t* init_libs = list_create();

    node_t* item;
    while( (item = list_pop(main_obj->dependencies)) )
    {
        while( end_addr * 0xFFF == 1 )
        {
            end_addr++;
        }

        char* lib_name = item->value;
        if( !strcmp(lib_name, "libg.so") )
        {
            free(item);
            continue;
        }

        elf_t* lib = open_object(lib_name);
        if( !lib )
        {
            //TODO stderr
            printf("Failed to load dependency '%s'\n", lib_name);
        }

        hashtable_set(libs, lib_name, lib);

        end_addr = object_load(lib, end_addr);
        object_postload(lib);
        object_relocate(lib);

        fclose(lib->file);

        if( lib->init_array )
        {
            list_insert(ctor_libs, lib);
        }
        if( lib->init )
        {
            list_insert(init_libs, lib);
        }

        lib->loaded = 1;

        free(item);
    }

    object_relocate(main_obj);
    while(end_addr & 0xFFF)
    {
        end_addr++;
    }

    char* ld_no_ctors = getenv("LD_DISABLE_CTORS");
    if( ld_no_ctors && (strcmp(ld_no_ctors,"1")) )
    {
        foreach(node, ctor_libs)
        {
            elf_t* lib = node->value;
            if( lib->init_array )
            {
                for( size_t i = 0; i < lib->init_array_size; i++ )
                {
                    lib->init_array[i]();
                }
            }
        }
    }

    foreach(node, init_libs)
    {
        elf_t* lib = node->value;
        lib->init();
    }

    if( main_obj->init_array )
    {
        for( size_t i = 0; i < main_obj->init_array_size; i++ )
        {
            main_obj->init_array[i]();
        }
    }

    if( main_obj->init )
    {
        main_obj->init();
    }

    main_obj->loaded = 1;

    char* args[] = {(char*)end_addr};
    setheap((uintptr_t)args[0]);

    if( hashtable_has(symbol_table, "malloc") ) 
    {
        _malloc = hashtable_get(symbol_table, "malloc");
    }
    if( hashtable_has(symbol_table, "free") ) 
    {
        _malloc = hashtable_get(symbol_table, "free");
    }
    _malloc_minimum = 0x40000000;

    entry_point_t entry = (entry_point_t)main_obj->header.e_entry;
    entry(argc-argc_offset, argv+argc_offset, environ);

    return 0;
}

//Locate library for LD_LIBRARY PATH
static char* find_lib( const char* file )
{
    //If it was an absolute path, there's no need to find it.
    if( strchr(file, '/') )
    {
        return strdup(file);
    }

    //Collect the environment variable.
    char* path = _target_is_suid ? NULL : getenv("LD_LIBRARY_PATH");
    if( !path )
    {
        //Not set - this is the default state. Should probably read from config file?
        path = "/lib";
    }

    //Duplicate so we can tokenize without editing
    char* xpath = strdup(path);
    char* p, *last;
    for( (p = strtok_r(xpath, ":", &last)); p; p = strtok_r(NULL, ":", &last) )
    {
        //Go through each LD_LIBRARY_PATH entry
        int r;
        struct stat stat_buf;

        //Append the requested file to that path
        char* exe = malloc(strlen(p) + strlen(file) + 2);
        *exe = '\0';
        strcat(exe, p);
        strcat(exe, "/");
        strcat(exe, file);

        //See if it exists
        r = stat(exe, &stat_buf);
        if( r != 0 )
        {
            // Nope.
            free(exe);
            continue;
        }

        //It exists, so this is what we want.
        return exe;
    }
    free(xpath);

    //No match found.
    return NULL;
}

//Open an object file
static elf_t* open_object( const char* path )
{
    if( !path )
    {
        return _main_obj;
    }

    if( hashtable_has(objects_table, (void*)path) )
    {
        elf_t* object = hashtable_get(objects_table, (void*)path);
        return object;
    }

    char* file = find_lib(path);
    if( !file )
    {
        return NULL;
    }

    FILE* f = fopen(file, "r");

    free(file);

    if( !f )
    {
        return NULL;
    }

    elf_t* object = malloc(sizeof(elf_t));
    memset(object, 0, sizeof(elf_t));
    hashtable_set(objects_table, (void*)path, object);

    if( !object )
    {
        return NULL;
    }

    object->file = f;

    size_t r = fread(&object->header, sizeof(Elf32_Header), 1, object->file);

    if( !r )
    {
        return NULL;
    }

    if( object->header.e_ident[0] != ELFMAG0 ||
        object->header.e_ident[1] != ELFMAG1 ||
        object->header.e_ident[2] != ELFMAG2 ||
        object->header.e_ident[3] != ELFMAG3 )
    {
        return NULL;
    }

    object->dependencies = list_create();

    return object;
}

//Calculate the size of an object file by examining it's phdrs
static size_t object_calculate_size( elf_t* object )
{
    uintptr_t base_addr = 0xFFFFFFFF;
    uintptr_t end_addr = 0x0;
    size_t headers = 0;

    while( headers < object->header.e_phnum )
    {
        Elf32_Phdr phdr;

        fseek(object->file, object->header.e_phoff + object->header.e_phentsize * headers, SEEK_SET);
        fread(&phdr, object->header.e_phentsize, 1, object->file);

        switch( phdr.p_type )
        {
            case PT_LOAD:
                if( phdr.p_vaddr < base_addr )
                {
                    base_addr = phdr.p_vaddr;
                }

                if( phdr.p_memsz + phdr.p_vaddr > end_addr )
                {
                    end_addr = phdr.p_memsz + phdr.p_vaddr;
                }
                break;
            default:
                break;
        }

        headers++;
    }

    if( base_addr == 0xFFFFFFFF )
    {
        return 0;
    }else
    {
        return end_addr - base_addr;
    }
}

//Load an object into memory
static uintptr_t object_load( elf_t* object, uintptr_t base )
{
    uintptr_t end_addr = 0x0;

    object->base = base;

    size_t headers = 0;
    while( headers < object->header.e_phnum )
    {
        Elf32_Phdr phdr;

        fseek(object->file, object->header.e_phoff + object->header.e_phentsize * headers, SEEK_SET);
        fread(&phdr, object->header.e_phentsize, 1, object->file);

        switch( phdr.p_type )
        {
            case PT_LOAD: ;//(a label can only be part of a statement and a declaration is not a statement)
                mmap(base + phdr.p_vaddr, phdr.p_memsz);

                fseek(object->file, phdr.p_offset, SEEK_SET);
                fread((void*)(base + phdr.p_vaddr), phdr.p_filesz, 1, object->file);

                size_t r = phdr.p_filesz;
                while( r < phdr.p_memsz )
                {
                    *(char*)(phdr.p_vaddr + base + r) = 0;
                    r++;
                }

                if( end_addr < phdr.p_vaddr + base + phdr.p_memsz )
                {
                    end_addr = phdr.p_vaddr + base + phdr.p_memsz;
                }
                break;
            case PT_DYNAMIC:
                object->dynamic = (Elf32_Dyn*)(base + phdr.p_vaddr);
                break;
            default:
                break;
        }

        headers++;
    }

    return end_addr;
}

//Perform cleanup after loading
static int object_postload( elf_t* object )
{
    if( object->dynamic )
    {
        Elf32_Dyn* table;

        table = object->dynamic;
        while( table->d_tag )
        {
            switch( table->d_tag )
            {
                case 4:
                    object->dyn_hash = (Elf32_Word*)(object->base + table->d_un.d_ptr);
                    object->dyn_symbol_table_size = object->dyn_hash[1];
                    break;
                case 5:
                    object->dyn_string_table = (char*)(object->base + table->d_un.d_ptr);
                    break;
                case 6:
                    object->dyn_symbol_table = (Elf32_Sym*)(object->base + table->d_un.d_ptr);
                    break;
                case 10:
                    object->dyn_string_table_size = table->d_un.d_val;
                    break;
                case 12:
                    object->init = (void (*)(void))(table->d_un.d_ptr + object->base);
                    break;
                case 25:
                    object->init_array = (void (**)(void))(table->d_un.d_ptr + object->base);
                    break;
                case 27:
                    object->init_array_size = table->d_un.d_val / sizeof(uintptr_t);
                    break;
            }

            table++;
        }

        table = object->dynamic;
        while( table->d_tag )
        {
            switch( table->d_tag )
            {
                case 1:
                    list_insert(object->dependencies, object->dyn_string_table + table->d_un.d_val);
                    break;
            }

            table++;
        }
    }

    return 0;
}

//Are symbol address needed for relocation type
static int need_symbol_for_type( unsigned char type )
{
    switch( type )
    {
        case 1:
        case 2:
        case 5:
        case 6:
        case 7:
            return 1;
        default:
            return 0;
    }
}

//Apply ELF relocations
static int object_relocate( elf_t* object )
{
    if( object->dyn_symbol_table )
    {
        Elf32_Sym* table = object->dyn_symbol_table;
        for( uintptr_t i = 0; i < (uintptr_t)object->dyn_symbol_table_size; i++ )
        {
            char* symname = (char*)((uintptr_t)object->dyn_string_table + table->st_name);

            if( !hashtable_has(symbol_table, symname) )
            {
                if( table->st_shndx )
                {
                    hashtable_set(symbol_table, symname, (void*)(table->st_value + object->base));
                }
            }

            table++;
        }
    }

    for( uintptr_t x = 0; x < object->header.e_shentsize * object->header.e_shnum; x += object->header.e_shentsize )
    {
        Elf32_Shdr shdr;

        fseek(object->file, object->header.e_shoff + x, SEEK_SET);
        fread(&shdr, object->header.e_shentsize, 1, object->file);

        if( shdr.sh_type == 9 )
        {
            Elf32_Rel* table = (Elf32_Rel*)(shdr.sh_addr + object->base);
            while( (uintptr_t)table - ((uintptr_t)shdr.sh_addr + object->base) < shdr.sh_size )
            {
                unsigned int symbol = ELF32_R_SYM(table->r_info);
                unsigned char type = ELF32_R_TYPE(table->r_info);
                Elf32_Sym* sym = &object->dyn_symbol_table[symbol];

                char* symname = NULL;
                uintptr_t x = sym->st_value + object->base;
                if( need_symbol_for_type(type) || (type == 5) )
                {
                    symname = (char*)((uintptr_t)object->dyn_string_table + sym->st_name);
                    if( symname && hashtable_has(symbol_table, symname) )
                    {
                        x = ((uintptr_t)hashtable_get(symbol_table, symname));
                    }else
                    {
                        x = 0x0;
                    }
                }

                switch(type)
                {
                    case 6: //GLOB_DAT
                        if( symname && hashtable_has(glob_dat, symname) )
                        {
                            x = (uintptr_t)hashtable_get(glob_dat, symname);
                        }
                    case 7: //JUMP_SLOT
                        memcpy((void*)(table->r_offset + object->base), &x, sizeof(uintptr_t));
                        break;
                    case 1: //32
                        x += *((ssize_t*)(table->r_offset + object->base));
                        memcpy((void*)(table->r_offset + object->base), &x, sizeof(uintptr_t));
                        break;
                    case 2: // PC32
                        x += *((ssize_t*)(table->r_offset + object->base));
                        x -= (table->r_offset + object->base);
                        memcpy((void*)(table->r_offset + object->base), &x, sizeof(uintptr_t));
                        break;
                    case 8: // RELATIVE
                        x = object->base;
                        x += *((ssize_t*)(table->r_offset + object->base));
                        memcpy((void*)(table->r_offset + object->base), &x, sizeof(uintptr_t));
                        break;
                    case 5: // COPY
                        memcpy((void*)(table->r_offset + object->base), (void*)x, sym->st_size);
                        break;
                }

                table++;
            }
        }
    }

    return 0;
}

//Copy relocations need to be located before other relocations
static void object_find_copy_relocations( elf_t* object )
{
    for( uintptr_t x = 0; x < object->header.e_shentsize * object->header.e_shnum; x += object->header.e_shentsize )
    {
        Elf32_Shdr shdr;

        fseek(object->file, object->header.e_shoff + x, SEEK_SET);
        fread(&shdr, object->header.e_shentsize, 1, object->file);

        if( shdr.sh_type == 9 )
        {
            Elf32_Rel* table = (Elf32_Rel*)(shdr.sh_addr + object->base);

            while((uintptr_t)table - ((uintptr_t)shdr.sh_addr + object->base) < shdr.sh_size)
            {
                unsigned char type = ELF32_R_TYPE(table->r_info);
                if( type == 5 )
                {
                    unsigned int symbol = ELF32_R_SYM(table->r_info);
                    Elf32_Sym* sym = &object->dyn_symbol_table[symbol];
                    char* symname = (char*)((uintptr_t)object->dyn_string_table + sym->st_name);
                    hashtable_set(glob_dat, symname, (void*)table->r_offset);
                }
                table++;
            }
        }
    }
}

//Find a symbol in a specific object
static void* object_find_symbol( elf_t* object, const char* symbol_name )
{
    if( !object->dyn_symbol_table )
    {
        return NULL;
    }

    Elf32_Sym* table = object->dyn_symbol_table;
    for( size_t i = 0; i < object->dyn_symbol_table_size; i++ )
    {
        if( !strcmp(symbol_name, (char*)((uintptr_t)object->dyn_string_table + table->st_name)) )
        {
            return (void*)(table->st_value + object->base);
        }
        table++;
    }

    return NULL;
}

//Fully load an object
static void* do_actual_load( const char* filename, elf_t* lib, int flags )
{
    if(!lib)
    {
        return NULL;
    }

    size_t lib_size = object_calculate_size(lib);

    if( lib_size < 4096 )
    {
        lib_size = 4096;
    }

    uintptr_t load_addr = (uintptr_t)malloc(lib_size);
    object_load(lib, load_addr);

    object_postload(lib);

    node_t* item;
    while( (item = list_pop(lib->dependencies)) )
    {
        elf_t* _lib = open_object(item->value);

        if( !_lib )
        {
            free((void*)load_addr);
            lib->loaded = 0;
            return NULL;
        }

        if( !_lib->loaded )
        {
            do_actual_load(item->value, _lib, 0);
        }
    }

    object_relocate(lib);

    fclose(lib->file);

    if( lib->init_array )
    {
        for( size_t i = 0; i < lib->init_array_size; i++ )
        {
            lib->init_array[i]();
        }
    }

    if( lib->init )
    {
        lib->init();
    }

    lib->loaded = 1;

    return (void*)lib;
}

static void* dlopen_ld( const char* filename, int flags )
{
    elf_t* lib = open_object(filename);

    if( !lib )
    {
        return NULL;
    }

    if( lib->loaded )
    {
        return lib;
    }

    void* ret = do_actual_load(filename, lib, flags);
    if( !ret )
    {
        hashtable_remove(objects_table, (void*)filename);
    }

    return ret;
}

static int dlclose_ld( elf_t* lib )
{
    free((void*)lib->base);
    return 0;
}

static char* dlerror_ld( void )
{
    return 0;
}
