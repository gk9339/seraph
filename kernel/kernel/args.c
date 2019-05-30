#include <kernel/kernel.h>
#include <string.h>
#include <stdlib.h>
#include <kernel/args.h>
#include <hashtable.h>
#include <kernel/serial.h>

hashtable_t* kernel_args_table = NULL;

static int tokenize( char* str, char* sep, char** buf )
{
    char* pch_i;
    char* save_i;
    int argc = 0;

    pch_i = strtok_r(str, sep, &save_i);
    if( !pch_i ) return 0;

    while( pch_i != NULL )
    {
        buf[argc] = (char*)pch_i;
        argc++;
        pch_i = strtok_r(NULL, sep, &save_i);
    }
    buf[argc] = NULL;

    return argc;
}

int args_present( char* karg )
{
    return hashtable_has(kernel_args_table, karg);
}

char* args_value( char* karg )
{
    return hashtable_get(kernel_args_table, karg);
}

void args_parse( char* cmdline )
{
    char* arg = strdup(cmdline);
    char* argv[1024];
    int argc = tokenize(arg, " ", argv);

    if( !kernel_args_table )
    {
        kernel_args_table = hashtable_create(10);
    }

    for( int i = 0; i < argc; i++ )
    {
        char* c = strdup(argv[i]);
        char* name;
        char* value;

        name = c;
        value = NULL;

        char* v = c;
        while( *v )
        {
            if( *v == '=' )
            {
                *v = '\0';
                v++;
                value = v;
                break;
            }
            v++;
        }

        hashtable_set(kernel_args_table, name, value);
    }
}
