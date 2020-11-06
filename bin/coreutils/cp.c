#include <stdio.h>
#include <getopt.h>
#include <stdlib.h>
#include <time.h>
#include <unistd.h>
#include <errno.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <string.h>
#include <dirent.h>

#define VERSION "0.1"

int interactive = 0;
int recursive = 0;
int follow = 1;
int verbose = 0;

void parse_args( int argc, char** argv ); // parse args, calling version/help functions
int cp( char* source, char* dest );
int cp_directory( char* source, char* dest, int mode, int uid, int gid );
int cp_file( char* source, char* dest, int mode, int uid, int gid );
int cp_symlink( char* source, char* dest, int mode, int uid, int gid );
void show_version( void ); // Display version text and exit
void show_usage( void ); // Display help text and exit

int main( int argc, char** argv )
{
    parse_args(argc, argv);

    if( optind < argc - 1 )
    {
        char* dest = argv[argc - 1];
        struct stat st;

        stat(dest, &st);
        if( S_ISDIR(st.st_mode) )
        {
            while( optind < argc - 1 )
            {
                char* source = strrchr(argv[optind], '/');
                if( !source ) source = argv[optind];
                
                char output[BUFSIZ];
                sprintf(output, "%s/%s", dest, source);

                cp(argv[optind], output);
                optind++;
            }
        }else
        {
            if( optind < argc - 2 )
            {
                fprintf(stderr, "%s: target '%s' is not a directory\n", argv[0], dest);
                return EXIT_FAILURE;
            }

            cp(argv[optind], dest);
        }
    }else
    {
        fprintf(stderr, "%s: not enough arguments\n", argv[0]);
        return EXIT_FAILURE;
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
            {"interactive", no_argument, 0, 'i'},
            {"recursive", no_argument, 0, 'r'},
            {"no-dereference", no_argument, 0, 'P'},
            {"verbose", no_argument, 0, 'v'},
            {"help", no_argument, 0, 'h'},
            {"version", no_argument, 0, 'V'},
            {0, 0, 0, 0}
        };

        int option_index = 0;

        c = getopt_long(argc, argv, "irRPv", long_options, &option_index);

        if( c == -1 )
        {
            break;
        }

        switch( c )
        {
            case 'i':
                interactive = 1;
                break;
            case 'r':
            case 'R':
                recursive = 1;
                break;
            case 'P':
                follow = 0;
                break;
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

int cp( char* source, char* dest )
{
    if( verbose )
    {
        printf("'%s' -> '%s'\n", source, dest);
    }

    struct stat st;
    
    if( follow )
    {
        lstat(source, &st);
    }else
    {
        stat(source, &st);
    }

    if( S_ISLNK(st.st_mode) )
    {
        return cp_symlink(source, dest, st.st_mode & 07777, st.st_uid, st.st_gid);
    }else if( S_ISDIR(st.st_mode) )
    {
        if( !recursive )
        {
            fprintf(stderr, "%s: %s: omitting directory\n", "cp", source);
            return 1;
        }
        return cp_directory(source, dest, st.st_mode & 07777, st.st_uid, st.st_gid);
    }else if( S_ISREG(st.st_mode) )
    {
        return cp_file(source, dest, st.st_mode & 07777, st.st_uid, st.st_gid);
    }else
    {
        fprintf(stderr, "%s: %s: %s\n", "cp", source, strerror(EFTYPE));
        return 1;
    }
}

int cp_directory( char* source, char* dest, int mode, int uid, int gid )
{
    DIR* dirp;
    
    if( interactive )
    {
        dirp = opendir(dest);
        if( dirp )
        {
            interactive = 2;
            char input[64];
            while( interactive == 2 )
            {
                printf("%s: overwrite directory '%s'? ", "cp", dest);
                fflush(stdout);
                fgets(input, 64, stdin);
                if( !strcmp(input, "y") || !strcmp(input, "y\n") )
                {
                    interactive = 1;
                    closedir(dirp);
                }else if( !strcmp(input, "n") || !strcmp(input, "n\n") )
                {
                    interactive = 1;
                    return EXIT_SUCCESS;
                    closedir(dirp);
                }
            }
        }
    }

    dirp= opendir(source);
    if( dirp == NULL )
    {
        fprintf(stderr, "Failed to open directory %s\n", source);
        return 1;
    }

    if( !strcmp( dest, "/" ) )
    {
        dest = "";
    }else
    {
        mkdir(dest, mode);
    }

    struct dirent* ent = readdir(dirp);
    while( ent != NULL )
    {
        if( !strcmp(ent->d_name, ".") || !strcmp(ent->d_name, "..") )
        {
            ent = readdir(dirp);
            continue;
        }

        char source_ent[strlen(source) + strlen(ent->d_name) + 2];
        sprintf(source_ent, "%s/%s", source, ent->d_name);
        char dest_ent[strlen(source) + strlen(ent->d_name) + 2];
        sprintf(dest_ent, "%s/%s", dest, ent->d_name);

        cp(source_ent, dest_ent);
        ent = readdir(dirp);
    }
    closedir(dirp);

    chmod(dest, mode);
    chown(dest, uid, gid);

    return 0;
}

int cp_file( char* source, char* dest, int mode, int uid, int gid )
{
    if( interactive && (access(dest, F_OK) != -1) )
    {
        interactive = 2;
        char input[64];
        while( interactive == 2 )
        {
            printf("%s: overwrite file '%s'? ", "cp", dest);
            fflush(stdout);
            fgets(input, 64, stdin);
            if( !strcmp(input, "y") || !strcmp(input, "y\n") )
            {
                interactive = 1;
            }else if( !strcmp(input, "n") || !strcmp(input, "n\n") )
            {
                interactive = 1;
                return EXIT_SUCCESS;
            }
        }   
    }

    int dest_fd = open(dest, O_WRONLY | O_CREAT | O_TRUNC, mode);
    int src_fd = open(source, O_RDONLY);

    ssize_t length;

    length = lseek(src_fd, 0, SEEK_END);
    lseek(src_fd, 0, SEEK_SET);

    char buf[BUFSIZ];

    while( length > 0 )
    {
        size_t r;
        r = read(src_fd, buf, length < BUFSIZ ? length : BUFSIZ);

        write(dest_fd, buf, r);
        length -= r;
    }

    close(dest_fd);
    close(src_fd);

    chmod(dest, mode);
    chown(dest, uid, gid);

    return 0;
}

int cp_symlink( char* source, char* dest, int mode, int uid, int gid )
{
    if( interactive && (access(dest, F_OK) != -1) )
    {
        interactive = 2;
        char input[64];
        while( interactive == 2 )
        {
            printf("%s: overwrite file '%s'? ", "cp", dest);
            fflush(stdout);
            fgets(input, 64, stdin);
            if( !strcmp(input, "y") || !strcmp(input, "y\n") )
            {
                interactive = 1;
            }else if( !strcmp(input, "n") || !strcmp(input, "n\n") )
            {
                interactive = 1;
                return EXIT_SUCCESS;
            }
        }   
    }

    char src_link[1024];

    readlink(source, src_link, 1024);

    symlink(src_link, dest);

    chmod(dest, mode);
    chown(dest, uid, gid);

    return 0;
}

// Display version text and exit
void show_version( void )
{
    printf("cp (\033[1;36mseraph\033[0m coreutils) %s\n", VERSION);

    exit(EXIT_SUCCESS);
}

// Display help text and exit
void show_usage( void )
{
    printf("Usage: cp SOURCE DEST\n"
           "Copy files and directories.\n\n"
           " -i, --interactive    prompt before overwriting\n"
           " -r,\n"
           " -R, --recursive      copy directories recursivley\n"
           " -P, --no-dereference never follow symbolic links in SOURCE\n"
           " -v, --verbose        output operations being performed\n"
           "     --help           display this help text and exit\n"
           "     --version        display version and exit\n");

    exit(EXIT_SUCCESS);
}
