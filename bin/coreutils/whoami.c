#include <stdio.h>
#include <getopt.h>
#include <stdlib.h>
#include <unistd.h>
#include <pwd.h>

#define VERSION "0.1"

void parse_args( int argc, char** argv ); // parse args, calling version/help functions
void show_version( void ); // Display version text and exit
void show_usage( void ); // Display help text and exit

int main( int argc, char** argv )
{
    parse_args(argc, argv);

    struct passwd* p = getpwuid(geteuid());

    if( !p )
    {
        return EXIT_FAILURE;
    }

    printf("%s\n", p->pw_name);

    endpwent();

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
            {"help", no_argument, 0, 'h'},
            {"version", no_argument, 0, 'V'},
            {0, 0, 0, 0}
        };

        int option_index = 0;

        c = getopt_long(argc, argv, "", long_options, &option_index);

        if( c == -1 )
        {
            break;
        }

        switch( c )
        {
            case 'V':
                show_version();
                __builtin_unreachable();
            case '?':
                fprintf(stderr, "Try 'whoami --help'\n");
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
    printf("whoami (\033[1;36mseraph\033[0m coreutils) %s\n", VERSION);

    exit(EXIT_SUCCESS);
}

// Display help text and exit
void show_usage( void )
{
    printf("Usage: whoami\n"
           "Print the username of the current effective user ID\n\n"
           "     --help           display this help text and exit\n"
           "     --version        display version and exit\n");

    exit(EXIT_SUCCESS);
}
