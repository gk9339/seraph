#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>
#include <string.h>
#include <errno.h>
#include <getopt.h>
#include <stdlib.h>
#include <sys/stat.h>
#include <ctype.h>

#define VERSION "0.2"

int nonprinting = 0;

void parse_args( int argc, char** argv ); // parse args, calling version/help functions
void cat( FILE* file, char* filename ); // Concatenate file to stdout
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
        cat(stdin, filename);
    }

    for( int i = optind; i < argc; i++ )
    {
        if( !strcmp(argv[i], "-") )
        {
            filename = "stdin";
            cat(stdin, filename);
            continue;
        }

        filename = argv[i];
        FILE* file = fopen(filename, "r");
        if( file == NULL )
        {
            fprintf(stderr, "%s: %s: %s\n", argv[0], argv[i], strerror(errno));
            retval = EXIT_FAILURE;
            continue;
        }

        struct stat st;
        fstat(fileno(file), &st);

        if( S_ISDIR(st.st_mode) )
        {
            fprintf(stderr, "%s: %s: Is a directory\n", argv[0], argv[i]);
            fclose(file);
            retval = EXIT_FAILURE;
            continue;
        }

        cat(file, filename);
        fclose(file);
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
            {"show-nonprinting", no_argument, 0, 'v'},
            {"help", no_argument, 0, 'h'},
            {"version", no_argument, 0, 'V'},
            {0, 0, 0, 0}
        };

        int option_index = 0;

        c = getopt_long(argc, argv, "vh", long_options, &option_index);

        if( c == -1 )
        {
            break;
        }

        switch( c )
        {
            case 'V':
                show_version();
                __builtin_unreachable();
            case 'v':
                nonprinting = 1;
                break;
            case '?':
                fprintf(stderr, "Try 'cat --help'\n");
                exit(EXIT_FAILURE);
            case 'h':
            default:
                show_usage();
                __builtin_unreachable();
        }
    }
}

// Concatenate file to stdout
void cat( FILE* file, char* filename )
{
    while( 1 )
    {
        char buf[BUFSIZ];
        char npbuf[BUFSIZ * 2];
        memset(buf, 0, BUFSIZ);

        ssize_t r = fread(buf, sizeof(char), BUFSIZ, file);
        if( !r )
        {
            return;
        }
        if( r < 0 )
        {
            fprintf(stderr, "cat: %s: %s\n", filename, strerror(errno));
            return;
        }

        if( !nonprinting )
        {
            fwrite(buf, sizeof(char), r, stdout);
        }else
        {
            int j = 0;
            for( int i = 0; i < r; i++ )
            {
                if( iscntrl(buf[i]) && buf[i] != '\n' && buf[i] != '\t' )
                {
                    if( buf[i] < 26 )
                    {
                        npbuf[j++] = '^';
                        npbuf[j++] = '@' + buf[i];
                    }else
                    {
                        npbuf[j++] = '^';
                        npbuf[j++] = '@';
                    }
                }else
                {
                    npbuf[j++] = buf[i];
                }
            }
            
            fwrite(npbuf, sizeof(char), r, stdout);
        }
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
           " -v, --show-nonprinting print control characters with ^ notation\n"
           " -h, --help             display this help text and exit\n"
           "     --version          display version and exit\n");

    exit(EXIT_SUCCESS);
}
