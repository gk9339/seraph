#include <sys/syscall.h>
#include <stdio.h>
#include <stdlib.h>

DEFN_SYSCALL1(exit, SYS_EXT, int)

extern void _init( void );
extern void _fini( void );

char** environ = NULL;
int _environ_size = 0;
char* _argv_0 = NULL;
char** __argv = NULL;

char** __get_argv( void )
{
    return __argv;
}

void _exit( int val )
{
    _fini();
    syscall_exit(val);

    __builtin_unreachable();
}

void _pre_main( int (*main)(int, char**), int argc, char* argv[] )
{
    if( !__get_argv() )
    {
        __argv = argv;
    }

    _init();
    _exit(main(argc, argv));
}
