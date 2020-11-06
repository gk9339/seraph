#include <stdio.h>
#include <getopt.h>
#include <stdlib.h>
#include <unistd.h>
#include <errno.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <string.h>

#define VERSION "0.1"

int interactive = 0;
int verbose = 0;

void parse_args( int argc, char** argv ); // parse args, calling version/help functions
int call( char** args ); // fork and exec args[]
void show_version( void ); // Display version text and exit
void show_usage( void ); // Display help text and exit

int main( int argc, char** argv )
{
    parse_args(argc, argv);

    if( optind < argc - 1 )
    {
        char* args[6]=
        {
            "/bin/cp",
            "-r",
            NULL
        };
        int argsc = 2;
        
        /*if( interactive )
        {
            args[argsc] = "-i";
            argsc++;
        }*/
        if( verbose )
        {
            args[argsc] = "-v";
            argsc++;
        }

        args[argsc] = argv[optind];
        args[argsc + 1] = argv[argc - 1];

        if( call(args) )
        {
            return 1;
        }
        args[0] = "/bin/rm";
        args[1] = "-r";
        args[2] = "-f";
        args[3] = argv[optind];
        args[4] = NULL;

        if( call(args) )
        {
            return 1;
        }
    }else
    {
        fprintf(stderr, "%s: not enough arguments\n", argv[0]);
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
            //{"interactive", no_argument, 0, 'i'},
            {"verbose", no_argument, 0, 'v'},
            {"help", no_argument, 0, 'h'},
            {"version", no_argument, 0, 'V'},
            {0, 0, 0, 0}
        };

        int option_index = 0;

        c = getopt_long(argc, argv, "fiv", long_options, &option_index);

        if( c == -1 )
        {
            break;
        }

        switch( c )
        {
            /*case 'i':
                interactive = 1;
                break;*/
            case 'v':
                verbose = 1;
                break;
            case 'V':
                show_version();
                __builtin_unreachable();
            case '?':
                fprintf(stderr, "Try 'cp --help'\n");
                exit(EXIT_FAILURE);
            case 'h':
            default:
                show_usage();
                __builtin_unreachable();
        }
    }
}

// fork and exec args[]
int call( char** args )
{
    pid_t pid = fork();
    if( !pid )
    {
        execvp(args[0], args);
        exit(1);
    }else
    {
        int status;
        waitpid(pid, &status, 0);
        return status;
    }
}

// Display version text and exit
void show_version( void )
{
    printf("mv (\033[1;36mseraph\033[0m coreutils) %s\n", VERSION);

    exit(EXIT_SUCCESS);
}

// Display help text and exit
void show_usage( void )
{
    printf("Usage: mv SOURCE DEST\n"
           "Moves files and directories.\n\n"
           //" -i, --interactive    prompt before overwriting"
           " -v, --verbose        output operations being performed\n"
           "     --help           display this help text and exit\n"
           "     --version        display version and exit\n");

    exit(EXIT_SUCCESS);
}
