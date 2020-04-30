#include <stdio.h>
#include <getopt.h>
#include <stdlib.h>
#include <unistd.h>
#include <libgen.h>
#include <errno.h>
#include <string.h>

#define VERSION "0.1"

int verbose = 0;
int force = 0;

void parse_args( int argc, char** argv ); // parse args, calling version/help functions
void show_version( void ); // Display version text and exit
void show_usage( void ); // Display help text and exit

int main( int argc, char** argv )
{
    parse_args(argc, argv);

    if( argc - optind < 1 )
    {
        fprintf(stderr, "Try 'ln --help'\n");
        exit(EXIT_FAILURE);
    }
    
    char* target = argv[optind];
    char* link_name = argv[optind + 1];
    if( !link_name )
    {
        link_name = strdup(target);
        link_name = basename(link_name);
    }

    if( force )
    {
        FILE* f;
        if( (f = fopen(link_name, "r")) )
        {
            fclose(f);
            unlink(link_name);
        }
    }

    if( symlink(target, link_name) < 0 )
    {
        fprintf(stderr, "%s: %s: %s\n", argv[0], link_name, strerror(errno));
        return EXIT_FAILURE;
    }

    if( verbose )
    {
        printf("%s -> %s\n", link_name, target);
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
            {"force", no_argument, 0, 'f'},
            {"symbolic", no_argument, 0, 's'},
            {"verbose", no_argument, 0, 'v'},
            {"help", no_argument, 0, 'h'},
            {"version", no_argument, 0, 'V'},
            {0, 0, 0, 0}
        };

        int option_index = 0;

        c = getopt_long(argc, argv, "fsv", long_options, &option_index);

        if( c == -1 )
        {
            break;
        }

        switch( c )
        {
            case 'f':
                force = 1;
                break;
            case 's':
                break;
            case 'v':
                verbose = 1;
                break;
            case 'V':
                show_version();
                __builtin_unreachable();
            case '?':
                fprintf(stderr, "Try 'ln --help'\n");
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
    printf("ln (\033[1;36mseraph\033[0m coreutils) %s\n", VERSION);

    exit(EXIT_SUCCESS);
}

// Display help text and exit
void show_usage( void )
{
    printf("Usage: ln [OPTION(s)] TARGET LINK_NAME\n"
           "create a link from LINK_NAME to TARGET.\n\n"
           " -f, --force    remove existing destination file\n"
           " -s, --symbolic exists for compatibilty, all links are symbolic\n"
           " -v, --verbose  print name of links\n"
           "     --help     display this help text and exit\n"
           "     --version  display version and exit\n");

    exit(EXIT_SUCCESS);
}
