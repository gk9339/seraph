#include <sys/types.h>
#include <dirent.h>
#include <sys/stat.h>
#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <libgen.h>
#include <limits.h>
#include <list.h>
#include <getopt.h>
#include <sys/ioctl.h>
#include <errno.h>
#include <pwd.h>
#include <grp.h>

#define VERSION "0.6"

#define FLAG_ALL        0x01
#define FLAG_ALMOST_ALL 0x02
#define FLAG_LONG       0x04
#define FLAG_HUMAN_SIZE 0x08

struct ls_entry
{
    char* filename;
    struct stat st;
    char* link;
    struct stat stlink;
    char* human_size;
};

int flags = 0;
int multiple = 0;
int is_tty = 0;
size_t line_length = 80;

void parse_args( int argc, char** argv ); // parse args, setting flags, or calling version/help functions
int display_dir( char* path ); // Display all files inside a dir
char* human_size( uint64_t st_size ); // Convert size in bytes into hman readable format
void display_files( struct ls_entry** ls_entry_array, int entries ); // Display each file in array
void print_entry( struct ls_entry* entry, int colwidth ); // print a file
void print_entry_long( struct ls_entry* entry, int size_width ); // Print a file (long / -l)
int printname_color( struct ls_entry* entry ); // Print filename with colour based on type
int num_places( int n ); // Count number of digits in a number
int files_before_dirs( const void* c1, const void* c2 ); // Comparison function 
static int filenames_alphabetical( const void* c1, const void* c2 ); // Comparison function
void show_version( void ); // Display version text and exit
void show_usage( void ); // Display help text and exit

int main( int argc, char** argv )
{
    char* path;
    int retval = 0;

    parse_args(argc, argv);

    if( argc > optind )
    {
        path = argv[optind];
        if( argc > optind + 1 )
        {
            multiple = 1;
        }
    }else
    {
        path = ".";
    }

    is_tty = isatty(STDOUT_FILENO);
    
    if( is_tty )
    {
        struct winsize ws;
        if( ioctl(STDOUT_FILENO, TIOCGWINSZ, &ws) != -1 && 0 < ws.ws_col && ws.ws_col == (size_t) ws.ws_col )
        {
            line_length = ws.ws_col;
        }
    }

    if( argc == 1 || optind == argc )
    {
        if( display_dir(path) == 2 )
        {
            fprintf(stderr, "%s: %s: %s\n", argv[0], path, strerror(errno));
            retval = 2;
        }
    }else
    {
        list_t* entry_list = list_create();
        while( path )
        {
            struct ls_entry* entry = malloc(sizeof(struct ls_entry));

            entry->filename = path;
            
            if( lstat(path, &entry->st) < 0 )
            {
                fprintf(stderr, "%s: %s: %s\n", argv[0], path, strerror(errno));
                free(entry);
                retval = 2;
            }else
            {
                if( S_ISLNK(entry->st.st_mode) )
                {
                    stat(path, &entry->stlink);
                    entry->link = malloc(4096);
                    readlink(path, entry->link, 4096);
                }
                list_insert(entry_list, entry);
            }

            optind++;
            if( optind >= argc )
            {
                path = NULL;
            }else
            {
                path = argv[optind];
            }
        }

        if( !entry_list->length )
        {
            return retval;
        }

        struct ls_entry** ls_entry_array = malloc(sizeof(struct ls_entry*) * entry_list->length);
        int index = 0;
        foreach(node, entry_list)
        {
            ls_entry_array[index++] = (struct ls_entry*)node->value;
        }

        list_free(entry_list);

        qsort(ls_entry_array, index, sizeof(struct ls_entry*), files_before_dirs);

        int first_dir = index;
 
        for( int i = 0; i < index; i++ )
        {
            if( S_ISDIR(ls_entry_array[i]->st.st_mode) || (S_ISLNK(ls_entry_array[i]->st.st_mode) && S_ISDIR(ls_entry_array[i]->stlink.st_mode)) )
            {
                first_dir = i;
                break;
            }
        }
 
        if( first_dir )
        {
            display_files(ls_entry_array, first_dir);
        }
 
        for( int i = first_dir; i < index; i++ )
        {
            if( i != 0 )
            {
                printf("\n");
            }
 
            if( display_dir(ls_entry_array[i]->filename) == 2 )
            {
                fprintf(stderr, "%s: %s: %s\n", argv[0], ls_entry_array[i]->filename, strerror(errno));
            }
        }
    }

    endpwent();
    endgrent();

    return retval;
}

// Parse args, setting flags, or calling version/help functions
void parse_args( int argc, char** argv )
{
    int c;

    while( 1 )
    {
        static struct option long_options[] =
        {
            {"all",            no_argument, 0, 'a'},
            {"almost-all",     no_argument, 0, 'A'},
            {"human-readable", no_argument, 0, 'h'},
            {"help",           no_argument, 0, 'H'},
            {"version",        no_argument, 0, 'v'},
            {0, 0, 0, 0}
        };

        int option_index = 0;

        c = getopt_long(argc, argv, "aAhl", long_options, &option_index);

        if( c == -1 )
        {
            break;
        }

        switch( c )
        {
            case 'a':
                flags |= FLAG_ALL;
                flags &= ~FLAG_ALMOST_ALL;
                break;
            case 'A':
                flags |= FLAG_ALMOST_ALL;
                flags &= ~FLAG_ALL;
                break;
            case 'h':
                flags |= FLAG_HUMAN_SIZE;
                break;
            case 'l':
                flags |= FLAG_LONG;
                break;
            case 'v':
                show_version();
                __builtin_unreachable();
            case '?':
                fprintf(stderr, "Try 'ls --help'\n");
                exit(EXIT_FAILURE);
            case 'H':
            default:
                show_usage();
                __builtin_unreachable();
        }
    }
}

// Display all files inside a dir
int display_dir( char* path )
{
    struct dirent* de;
    DIR* dr = NULL;
    char* filename;
    struct ls_entry* entry;
    list_t* ls_list;

    dr = opendir(path);
    int pathlen = strlen(path);

    if( dr == NULL )
    {
        // Opendir failed
        printf("\033[0;31mCouldn't open %s.\n", path); // Red
        return 2;
    }else
    {
        if( multiple )
        {
            if( is_tty )
            {
                printf("\033[1;34m%s\033[1;32m:\033[0m\n", path);
            }else
            {
                printf("%s:\n", path);
            }
        }

        ls_list = list_create();
        while( (de = readdir(dr)) != NULL )
        {
            filename = de->d_name;

            if( (!(flags & FLAG_ALL) && !(flags & FLAG_ALMOST_ALL)) && filename[0] == '.' )
            {
                // Not all or almost all, ignore files that start with .
                continue;
            }
            if( flags & FLAG_ALMOST_ALL && (!strcmp(filename, ".") || !strcmp(filename, "..")) )
            {
                // Almost all, ignore . and ..
                continue;
            }

            char* filepath = malloc(pathlen + strlen(filename) + 2);
            sprintf(filepath, "%s/%s", path, filename);
            entry = malloc(sizeof(struct ls_entry));

            if( lstat(filepath, &entry->st) < 0 )
            {
                // lstat failed
                free(filepath);
                free(entry);
                continue;
            }
            if( S_ISLNK(entry->st.st_mode) )
            {
                stat(filepath, &entry->stlink);
                entry->link = malloc(4096);
                readlink(filepath, entry->link, 4096);
            }
            free(filepath);

            entry->filename = malloc(strlen(filename) + 1);
            strcpy(entry->filename, filename);

            list_insert(ls_list, entry);
        }
        closedir(dr);
    }

    if( !ls_list->length )
    {
        return 0;
    }

    struct ls_entry** ls_entry_array = malloc(sizeof(struct ls_entry*) * ls_list->length);
    int index = 0;
    foreach(node, ls_list)
    {
        ls_entry_array[index++] = (struct ls_entry*)node->value;
    }

    list_free(ls_list);

    qsort(ls_entry_array, index, sizeof(struct ls_entry*), filenames_alphabetical);

    display_files(ls_entry_array, index);

    free(ls_entry_array);

    return 0;
}

// Convert size in bytes into hman readable format
char* human_size( uint64_t st_size )
{
    char* output;

    if( st_size != 0 )
    {
        char* suffix[] = { "B", "K", "M", "G", "T" };
        char length = sizeof(suffix) / sizeof(suffix[0]);
 
        int i = 0;
        double fp_size = st_size;
 
        if( st_size > 1024 )
        {
            for( i = 0; (st_size / 1024) > 0 && i < length - 1; i++, st_size /= 1024 )
            {
                fp_size = st_size / 1024.0;
            }
        }
 
        char fmt[64];
        output = malloc((1 + sprintf(fmt, "%4.2lf %s", fp_size, suffix[i])) * sizeof(char));
        strcpy(output, fmt);
    }else
    {
        char fmt[64];
        output = malloc((1 + sprintf(fmt, "0 B")) * sizeof(char));
        strcpy(output, fmt);
    }

    return output;
}

// Display each file in array
void display_files( struct ls_entry** ls_entry_array, int entries )
{
    if( flags & FLAG_LONG )
    {
        int size_width = 0;

        if( flags & FLAG_HUMAN_SIZE )
        {
            for( int i = 0; i < entries; i++ )
            {
                ls_entry_array[i]->human_size = human_size(ls_entry_array[i]->st.st_size);
                int width = strlen(ls_entry_array[i]->human_size);
                if( width > size_width )
                {
                    size_width = width;
                }
            }
        }else
        {
            for( int i = 0; i < entries; i++ )
            {
                int width = num_places(ls_entry_array[i]->st.st_size);
                if( width > size_width )
                {
                    size_width = width;
                }
            }   
        }

        for( int i = 0; i < entries; i++ )
        {
            print_entry_long(ls_entry_array[i], size_width);
        }
    }else
    {
        int max_file_len = 0;
        int file_len = 0;
        for( int i = 0; i < entries; i++ )
        {
            if( is_tty )
            {
                file_len = strlen(ls_entry_array[i]->filename);
                if( S_ISDIR(ls_entry_array[i]->st.st_mode) || (S_ISLNK(ls_entry_array[i]->st.st_mode) && (S_ISDIR(ls_entry_array[i]->stlink.st_mode))) )
                {
                    file_len++;
                }else if( S_ISREG(ls_entry_array[i]->st.st_mode) )
                {
                    if( ls_entry_array[i]->st.st_mode & S_IXUSR || ls_entry_array[i]->st.st_mode & S_IXGRP || ls_entry_array[i]->st.st_mode & S_IXOTH )
                    {
                        file_len++;
                    }
                }

                if( file_len > max_file_len )
                {
                    max_file_len = file_len;
                }
            }else
            {
                file_len = strlen(ls_entry_array[i]->filename);
                if( file_len > max_file_len )
                {
                    max_file_len = file_len;
                }
            }
        }

        int cols = ((line_length - max_file_len) / (max_file_len + 2)) + 1;

        int i = 0;
        while( i < entries )
        {
            print_entry(ls_entry_array[i++], max_file_len);

            for( int j = 0; (i < entries) && (j < (cols - 1)); j++ )
            {
                printf("  ");
                print_entry(ls_entry_array[i++], max_file_len);
            }

            printf("\n");
        }
    }
}

// Display each file in array
void print_entry( struct ls_entry* entry, int colwidth )
{
    int len;
    if( is_tty )
    {
        len = printname_color(entry);
    }else
    {
        printf("%s", entry->filename);
        len = strlen(entry->filename);
    }

    for( int i = colwidth - len; i > 0; i-- )
    {
        printf(" ");
    }
}

// Print a file (long / -l)
void print_entry_long( struct ls_entry* entry, int size_width )
{
    mode_t mode;
    char perm[] = "----------\0";

    mode = entry->st.st_mode;

    if( S_ISLNK(mode) )
    {
        perm[0] = 'l';
    }else if( S_ISCHR(mode) )
    {
        perm[0] = 'c';
    }else if( S_ISDIR(mode) )
    {
        perm[0] = 'd';
    }else if( S_ISFIFO(mode) )
    {
        perm[0] = 'f';
    }else if( S_ISREG(mode) )
    {
        perm[0] = '-';
    }else if( S_ISBLK(mode) )
    {
        perm[0] = 'b';
    }else if( S_ISSOCK(mode) )
    {
        perm[0] = 's';
    }
    perm[1] = (mode & S_IRUSR) ? 'r' : '-';
    perm[2] = (mode & S_IWUSR) ? 'w' : '-';
    if( mode & S_ISUID )
    {
        perm[3] = 's';
    }else
    {
        perm[3] = (mode & S_IXUSR) ? 'x' : '-';
    }
    perm[4] = (mode & S_IRGRP) ? 'r' : '-';
    perm[5] = (mode & S_IWGRP) ? 'w' : '-';
    perm[6] = (mode & S_IXGRP) ? 'x' : '-';
    perm[7] = (mode & S_IROTH) ? 'r' : '-';
    perm[8] = (mode & S_IWOTH) ? 'w' : '-';
    perm[9] = (mode & S_IXOTH) ? 'x' : '-';

    printf("%s %s %s ", perm, getpwuid(entry->st.st_uid)->pw_name, getgrgid(entry->st.st_gid)->gr_name);
    if( flags & FLAG_HUMAN_SIZE )
    {
        printf("%*s ", size_width, entry->human_size);
        free(entry->human_size);
    }else
    {
        printf("%*zdB ", size_width, entry->st.st_size);
    }

    printname_color(entry);
    printf("\n");
}

// Print filename with colour based on type
int printname_color( struct ls_entry* entry )
{
    int retval = strlen(entry->filename);

    if( S_ISCHR(entry->st.st_mode) )
    {
        printf("\033[48;5;202m%s", entry->filename); // Yellow/Orange BG (Character device)
    }else if( S_ISDIR(entry->st.st_mode) )
    {
        printf("\033[38;5;30m%s\033[0;0m/", entry->filename); // Blue/Green (Directory)
        retval++;
    }else if( S_ISFIFO(entry->st.st_mode) )
    {
        printf("\033[1;41m%s", entry->filename); // Red BG (FIFO)
    }else if( S_ISREG(entry->st.st_mode) )
    {
        if( entry->st.st_mode & S_IXUSR || entry->st.st_mode & S_IXGRP || entry->st.st_mode & S_IXOTH )
        {
            printf("\033[38;5;208m%s\033[38;5;2m*", entry->filename); // Executable file
            retval++;
        }else
        {
            printf("%s", entry->filename); // Default (Regular file)
        }
    }else if( S_ISBLK(entry->st.st_mode) )
    {
        printf("\033[48;5;89m%s", entry->filename); // Magenta BG (Block device)
    }else if( S_ISSOCK(entry->st.st_mode) )
    {
        printf("\033[0;31m%s", entry->filename); // Red (Socket)
    }else

    if( flags & FLAG_LONG && S_ISLNK(entry->st.st_mode) )
    {
        struct ls_entry lnk =
        {
            .filename = entry->filename,
            .st = entry->stlink,
            .link = NULL,
            .stlink = {0},
        };
        printname_color(&lnk);
        printf(" -> ");
        lnk.filename = entry->link;
        printname_color(&lnk);
    }else if( S_ISLNK(entry->st.st_mode) )
    {
        struct ls_entry lnk =
        {
            .filename = entry->filename,
            .st = entry->stlink,
            .link = NULL,
            .stlink = {0},
        };
        retval = printname_color(&lnk);
    }else
    {
        printf("\033[1;41m%s", entry->filename); // Red BG (broken symlink)
    }

    printf("\033[0m");

    return retval;
}

// Count number of digits in a number
int num_places( int n )
{
    int r = 1;
    if( n < 0 )
    {
        n = (n == INT_MIN) ? INT_MAX: -n;
    }

    while( n > 9 )
    {
        n /= 10;
        r++;
    }

    return r;
}

// Comparison function
int files_before_dirs( const void* c1, const void* c2 )
{
    const struct ls_entry* d1 = *(const struct ls_entry**)c1;
    const struct ls_entry* d2 = *(const struct ls_entry**)c2;

    int a = S_ISDIR(d1->st.st_mode);
    int b = S_ISDIR(d2->st.st_mode);

    if( a == b )
    {
        return strcmp(d1->filename, d2->filename);
    }
    else if( a < b )
    {
        return -1;
    }
    else if( a > b )
    {
        return 1;
    }

    return 0;
}

// Comparison function
static int filenames_alphabetical( const void* c1, const void* c2 )
{
    const struct ls_entry* d1 = *(const struct ls_entry**)c1;
    const struct ls_entry* d2 = *(const struct ls_entry**)c2;

    return strcmp(d1->filename, d2->filename);
}

// Display version text and exit
void show_version( void )
{
    printf("ls (\033[1;36mseraph\033[0;m coreutils) %s\n", VERSION);

    exit(EXIT_SUCCESS);
}

// Display help text and exit
void show_usage( void )
{
    printf("Usage: ls [OPTION(s)] [FILE(s)]\n"
           "List information about FILE(s), or current directory by default\n\n"
           " -a, --all        do not ignore files starting with .\n"
           " -A, --almost-all same as all, without '.' and '..'\n"
           " -l               long listing format\n"
           "     --help       display this help text and exit\n"
           "     --version    display version and exit\n");

    exit(EXIT_SUCCESS);
}
