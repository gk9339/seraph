#include <sys/syscall.h>
#include <stdio.h>
#include <stdlib.h>

DEFN_SYSCALL1(exit, SYS_EXT, int)

extern void _init( void );
extern void _fini( void );
extern void __stdio_init_buffers( void );

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

static void __attribute__((constructor)) _libc_init( void )
{
    __stdio_init_buffers();

    unsigned int x = 0;
	unsigned int nulls = 0;
	for( x = 0; 1; x++ )
    {
		if( !__get_argv()[x] )
        {
			nulls++;
			if( nulls == 2 )
            {
				break;
			}
			continue;
		}
		if( nulls == 1 )
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
		/* Find actual size */
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
			/* Multiply by two */
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
}
