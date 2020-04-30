#include <stdio.h>
#include <getopt.h>
#include <stdlib.h>
#include <unistd.h>
#include <string.h>
#include <errno.h>

#define VERSION "0.1"

int ignore = 0;

void parse_args( int argc, char** argv ); // parse args, calling version/help functions
void show_version( void ); // Display version text and exit
void show_usage( void ); // Display help text and exit

int main( int argc, char** argv )
{
    parse_args(argc, argv);
    
    if( ignore )
    {
        int i = 0;
        while( environ[i] )
        {
            environ[i] = NULL;
            i++;
        }
    }

    for(; optind < argc; optind++ )
    {
        if( !strchr(argv[optind],'=') )
        {
            break;
        }else
        {
            putenv(argv[optind]);
        }
    }

    if( optind < argc )
    {
        if( execvp(argv[optind], &argv[optind]) )
        {
            fprintf(stderr, "%s: %s: %s\n", argv[0], argv[optind], strerror(errno));
        }
    }else
    {
        char** env = environ;

        while( *env )
        {
            printf("%s\n", *env);
            env++;
        }
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
            {"ignore-environment", no_argument, 0, 'i'},
            {"help", no_argument, 0, 'h'},
            {"version", no_argument, 0, 'v'},
            {0, 0, 0, 0}
        };

        int option_index = 0;

        c = getopt_long(argc, argv, "i", long_options, &option_index);

        if( c == -1 )
        {
            break;
        }

        switch( c )
        {
            case 'i':
                ignore = 1;
                break;
            case 'v':
                show_version();
                __builtin_unreachable();
            case '?':
                fprintf(stderr, "Try 'env --help'\n");
                exit(EXIT_FAILURE);
            case 'h':
            default:
                show_usage();
                __builtin_unreachable();
        }
    }
}

// Display version text and exit
void show_version( void )
{
    printf("env (\033[1;36mseraph\033[0m coreutils) %s\n", VERSION);

    exit(EXIT_SUCCESS);
}

// Display help text and exit
void show_usage( void )
{
    printf("Usage: env [OPTION(s)] [NAME(s)=VALUE] [COMMAND [ARG]]\n"
           "Set each NAME to VALUE in the environment and run COMMAND.\n\n"
           " -i, --ignore-environment start with an empty environment\n"
           "     --help               display this help text and exit\n"
           "     --version            display version and exit\n");

    exit(EXIT_SUCCESS);
}
