#include <stdio.h>
#include <stdlib.h>

extern void _init( void );
extern void _fini( void );

char** __get_argv( void );
void _pre_main( int (*main)(int, char**), int argc, char* argv[] );
void _exit( int val );

char** environ = NULL;
int _environ_size = 0;
char* _argv_0 = NULL;
char** __argv = NULL;

char** __get_argv( void )
{
    return __argv;
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
