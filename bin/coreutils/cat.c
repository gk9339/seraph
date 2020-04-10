#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>
#include <string.h>
#include <errno.h>
#include <getopt.h>
#include <stdlib.h>
#include <sys/stat.h>

#define VERSION "0.1"

void parse_args( int argc, char** argv ); // parse args, calling version/help functions
void cat( int fd, char* filename ); // Concatenate file to stdout
void show_version( void ); // Display version text and exit
void show_usage( void ); // Display help text and exit

int main( int argc, char** argv )
{
    int retval = 0;
    char* filename;

    parse_args(argc, argv);

    if( argc == 1 )
    {
        filename = "stdin";
        cat(STDIN_FILENO, filename);
    }

    for( int i = 1; i < argc; i++ )
    {
        if( !strcmp(argv[i], "-") )
        {
            filename = "stdin";
            cat(STDIN_FILENO, filename);
            continue;
        }

        filename = argv[i];
        int fd = open(filename, O_RDONLY);
        if( fd < 0 )
        {
            fprintf(stderr, "%s: %s: %s\n", argv[0], argv[i], strerror(errno));
            retval = 1;
            continue;
        }

        struct stat st;
        fstat(fd, &st);

        if( S_ISDIR(st.st_mode) )
        {
            fprintf(stderr, "%s: %s: Is a directory\n", argv[0], argv[i]);
            close(fd);
            retval = 1;
            continue;
        }

        cat(fd, filename);
        close(fd);
    }

    return retval;
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
            {"version", no_argument, 0, 'v'},
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
            case 'v':
                show_version();
                __builtin_unreachable();
            case '?':
                fprintf(stderr, "Try 'uname --help'\n");
                exit(EXIT_FAILURE);
            case 'h':
            default:
                show_usage();
                __builtin_unreachable();
        }
    }
}

// Concatenate file to stdout
void cat( int fd, char* filename )
{
    while( 1 )
    {
        char buf[BUFSIZ];
        memset(buf, 0, BUFSIZ);

        ssize_t r = read(fd, buf, BUFSIZ);
        if( !r )
        {
            return;
        }
        if( r < 0 )
        {
            fprintf(stderr, "cat: %s: %s\n", filename, strerror(errno));
            return;
        }

        write(STDOUT_FILENO, buf, r);
    }
}

// Display version text and exit
void show_version( void )
{
    printf("cat (\033[1;36mseraph\033[0m coreutils) %s\n", VERSION);

    exit(EXIT_SUCCESS);
}

// Display help text and exit
void show_usage( void )
{
    printf("Usage: cat [FILE(s)]\n"
           "Concatenate FILE(s) to standard output.\n\n"
           "     --help    display this help text and exit\n"
           "     --version display version and exit\n");

    exit(EXIT_SUCCESS);
}
