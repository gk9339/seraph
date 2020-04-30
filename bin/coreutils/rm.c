#include <stdio.h>
#include <getopt.h>
#include <stdlib.h>
#include <unistd.h>
#include <string.h>
#include <sys/stat.h>
#include <dirent.h>
#include <errno.h>
#include <list.h>

#define VERSION "0.1"

int force = 0;
int recursive = 0;
int verbose = 0;

void parse_args( int argc, char** argv ); // parse args, calling version/help functions
int rm( char* name ); // Remove file 'name'
int rm_dir( char* name ); // Remove directory 'name'
void show_version( void ); // Display version text and exit
void show_usage( void ); // Display help text and exit

int main( int argc, char** argv )
{
    int retval = EXIT_SUCCESS;

    parse_args(argc, argv);
    
    for( int i = optind; i < argc; i++ )
    {
        if( rm(argv[i]) )
        {
            fprintf(stderr, "%s: %s: %s\n", argv[0], argv[i], strerror(errno));
            retval = EXIT_FAILURE;
        }
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
            {"force", no_argument, 0, 'f'},
            {"recursive", no_argument, 0, 'r'},
            {"verbose", no_argument, 0, 'v'},
            {"help", no_argument, 0, 'h'},
            {"version", no_argument, 0, 'V'},
            {0, 0, 0, 0}
        };

        int option_index = 0;

        c = getopt_long(argc, argv, "frRv", long_options, &option_index);

        if( c == -1 )
        {
            break;
        }

        switch( c )
        {
            case 'f':
                force = 1;
                break;
            case 'r':
            case 'R':
                recursive = 1;
                break;
            case 'v':
                verbose = 1;
                break;
            case 'V':
                show_version();
                __builtin_unreachable();
            case '?':
                fprintf(stderr, "Try 'rm --help'\n");
                exit(EXIT_FAILURE);
            case 'h':
            default:
                show_usage();
                __builtin_unreachable();
        }
    }
}

// Remove file 'name'
int rm( char* name )
{
    struct stat st;
    if( lstat(name, &st) < 0 )
    {
        if( !force )
        {
            fprintf(stderr, "%s: %s: %s\n", "rm", name, strerror(errno));
            return EXIT_FAILURE;
        }else
        {
            return EXIT_SUCCESS;
        }
    }
    if( S_ISDIR(st.st_mode) )
    {
        if( !recursive )
        {
            fprintf(stderr, "%s: %s: is a directory\n", "rm", name);
            return EXIT_FAILURE;
        }
        return rm_dir(name);
    }else
    {
        if( verbose )
        {
            printf("removed %s\n", name);
        }
        return unlink(name);
    }
}

// Remove directory 'name'
int rm_dir( char* name )
{
    DIR* dirp = opendir(name);
    if( dirp == NULL )
    {
        if( !force )
        {
            fprintf(stderr, "%s: %s: %s\n", "rm", name, strerror(errno));
            return EXIT_FAILURE;
        }else
        {
            return EXIT_SUCCESS;
        }
    }

    list_t* entry_list = list_create();
    struct dirent* ent = readdir(dirp);
    while( ent != NULL )
    {
        if( !strcmp(ent->d_name, ".") || !strcmp(ent->d_name, "..") )
        {
            ent = readdir(dirp);
            continue;
        }

        char file[strlen(name) + strlen(ent->d_name) + 2];
        sprintf(file, "%s/%s", name, ent->d_name);
        list_insert(entry_list, strdup(file));

        ent = readdir(dirp);
    }
    closedir(dirp);
    
    foreach(node, entry_list)
    {
        char* file = (char*)node->value;
        
        if( rm(file) )
        {
            free(file);
            return EXIT_FAILURE;
        }
        free(file);
    }

    if( verbose )
    {
        printf("removed directory %s\n", name);
    }
    return unlink(name);
}

// Display version text and exit
void show_version( void )
{
    printf("rm (\033[1;36mseraph\033[0m coreutils) %s\n", VERSION);

    exit(EXIT_SUCCESS);
}

// Display help text and exit
void show_usage( void )
{
    printf("Usage: rm [OPTION(s)] FILE(s)\n"
           "Remove (unlink) FILE(s).\n\n"
           " -f, --force     ignore nonexistant files and arguments, never prompt\n"
           " -r,\n"
           " -R, --recursive remove directories and their contents recursivley\n"
           " -v, --verbose   output actions being performed\n"
           "     --help      display this help text and exit\n"
           "     --version   display version and exit\n");

    exit(EXIT_SUCCESS);
}
