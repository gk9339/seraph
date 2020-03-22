#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

extern void _init( void );
extern void _fini( void );
extern void __stdio_init_buffers( void );

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

__attribute__((constructor)) static void _libc_init( void )
{
    __stdio_init_buffers();

    unsigned int x = 0;
    unsigned int nulls = 0;
    
    while( x++ )
    {
        if( !__get_argv()[x] )
        {
            nulls++;
            if( nulls == 2 )
            {
                break;
            }
        }else if( nulls == 1)
        {
            environ = &__get_argv()[x];
            break;
        }
    }

    if( !environ )
    {
        environ = malloc(sizeof(char*) * 4);
        environ[0] = NULL;
        environ[1] = NULL;
        environ[2] = NULL;
        environ[3] = NULL;
        _environ_size = 4;
    }else
    {
        int size = 0;

        char** tmp = environ;
        while( *tmp )
        {
            size++;
            tmp++;
        }

        if( size < 4 )
        {
            _environ_size = 4;
        }else
        {
            _environ_size = size * 2;
        }

        char** new_environ = malloc(sizeof(char*) * _environ_size);
        int i = 0;
        while( i < _environ_size && environ[i] )
        {
            new_environ[i] = environ[i];
            i++;
        }

        while( i < _environ_size )
        {
            new_environ[i] = NULL;
            i++;
        }

        environ = new_environ;
    }

    _argv_0 = __get_argv()[0];
}
