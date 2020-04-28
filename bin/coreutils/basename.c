#include <stdio.h>
#include <getopt.h>
#include <stdlib.h>
#include <unistd.h>
#include <libgen.h>
#include <string.h>

#define VERSION "0.1"

char* suffix = NULL;
int multiple = 0;
int zero = 0;

void parse_args( int argc, char** argv ); // parse args, calling version/help functions
void basename_func( char* string, int zero );
void show_version( void ); // Display version text and exit
void show_usage( void ); // Display help text and exit

int main( int argc, char** argv )
{
    parse_args(argc, argv);

    if( argc < optind + 1 )
    {
        fprintf(stderr, "%s: missing argument\n", argv[0]);
        exit(EXIT_FAILURE);
    }

    if( !multiple && optind + 2 < argc )
    {
        fprintf(stderr, "%s: extra operand %s\n", argv[0], argv[optind + 2]);
        exit(EXIT_FAILURE);
    }

    if( multiple )
    {
        for(; optind < argc; optind++ )
        {
            basename_func(argv[optind],  zero);
        }
    }else
    {
        basename_func(argv[optind], zero);
    }

    return EXIT_SUCCESS;
}

// parse args, calling version/help functions
void parse_args( int argc, char** argv )
{
    int c;

    while( 1 )
    {
        static struct option long_options[] =
        {
            {"multiple", no_argument, 0, 'a'},
            {"suffix", required_argument, 0, 's'},
            {"zero", no_argument, 0, 'z'},
            {"help", no_argument, 0, 'h'},
            {"version", no_argument, 0, 'v'},
            {0, 0, 0, 0}
        };

        int option_index = 0;

        c = getopt_long(argc, argv, "+as:z", long_options, &option_index);

        if( c == -1 )
        {
            break;
        }

        switch( c )
        {
            case 's':
                suffix = optarg;
                __attribute__((fallthrough));
            case 'a':
                multiple = 1;
                break;
            case 'z':
                zero = 1;
                break;
            case 'v':
                show_version();
                __builtin_unreachable();
            case '?':
                fprintf(stderr, "Try 'basename --help'\n");
                exit(EXIT_FAILURE);
            case 'h':
            default:
                show_usage();
                __builtin_unreachable();
        }
    }
}

void basename_func( char* string, int zero )
{
    char* name = basename(string);

    if( suffix )
    {
        char* np;
        const char* sp;

        np = name + strlen(name);
        sp = suffix + strlen(suffix);

        while( np > name && sp > suffix )
        {
            if( *--np != *--sp )
            {
                break;
            }
        }

        if( np > name )
        {
            *np = '\0';
        }
    }

    fputs(name, stdout);
    putchar(zero ? '\0' : '\n');
    free(name);
}

// Display version text and exit
void show_version( void )
{
    printf("basename (\033[1;36mseraph\033[0m coreutils) %s\n", VERSION);

    exit(EXIT_SUCCESS);
}

// Display help text and exit
void show_usage( void )
{
    printf("Usage: basename NAME [SUFFIX]\n"
           "   or: basename OPTION(s) NAME(s)\n"
           "Print NAME with leading directory components removed to standard output.\n\n"
           " -a, --multiple      support multiple NAME(s)\n"
           " -s, --suffix=SUFFIX remove a trailing SUFFIX; implies -a\n"
           " -z, --zero          end each output with NUL instead of '\\n'\n"
           "     --help          display this help text and exit\n"
           "     --version       display version and exit\n");

    exit(EXIT_SUCCESS);
}
