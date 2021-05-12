#include <stdio.h>
#include <getopt.h>
#include <stdlib.h>
#include <unistd.h>
#include <pwd.h>
#include <grp.h>
#include <ctype.h>
#include <sys/types.h>

#define VERSION "0.2"

int group = 0;
int name = 0;
int user = 0;
int zero = 0;

void parse_args( int argc, char** argv ); // parse args, calling version/help functions
void id( struct passwd* pw ); // print user and group information, depending on options
int is_number( char* s ); // Checks if string is a number
void show_version( void ); // Display version text and exit
void show_usage( void ); // Display help text and exit

int main( int argc, char** argv )
{
    int retval = EXIT_SUCCESS;
    struct passwd* pw;
    parse_args(argc, argv);

    if( name && (!group && !user) )
    {
        printf("%s: cannot print only names in default format\n", argv[0]);
        return EXIT_FAILURE;
    }

    if( zero && (!group && !user) )
    {
        printf("%s: cannot use --zero in default format\n", argv[0]);
        return EXIT_FAILURE;
    }

    if( argc == 1 || optind == argc )
    {
        pw = getpwuid(geteuid());
        if( pw )
        {
            id(pw);
        }else
        {
            printf("%s: could not get current user\n", argv[0]);
        }
    }   

    for( int i = optind; i < argc; i++ )
    {
        pw = is_number(argv[i])? getpwuid(strtod(argv[i], NULL)) : getpwnam(argv[i]);
        if( pw )
        {
            id(pw);
        }else
        {
            printf("%s: \'%s\': no such user\n", argv[0], argv[i]);
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
            {"group", no_argument, 0, 'g'},
            {"groups", no_argument, 0, 'G'},
            {"name", no_argument, 0, 'n'},
            {"user", no_argument, 0, 'u'},
            {"zero", no_argument, 0, 'z'},
            {"help", no_argument, 0, 'h'},
            {"version", no_argument, 0, 'V'},
            {0, 0, 0, 0}
        };

        int option_index = 0;

        c = getopt_long(argc, argv, "gGnuz", long_options, &option_index);

        if( c == -1 )
        {
            break;
        }

        switch( c )
        {
            case 'a':
                break;
            case 'g':
                group = 1;
                user = 0;
                break;
            case 'G':
                group = 2;
                user = 0;
                break;
            case 'n':
                name = 1;
                break;
            case 'u':
                user = 1;
                group = 0;
                break;
            case 'z':
                zero = 1;
                break;
            case 'V':
                show_version();
                __builtin_unreachable();
            case '?':
                fprintf(stderr, "Try 'id --help'\n");
                exit(EXIT_FAILURE);
            case 'h':
            default:
                show_usage();
                __builtin_unreachable();
        }
    }
}

// print user and group information, depending on options
void id( struct passwd* pw )
{
    if( group == 1 )
    {
        if( name )
        {
            printf("%s", getgrgid(pw->pw_gid)->gr_name);
        }else
        {
            printf("%d", pw->pw_gid);
        }
    }else if( group == 2 )
    {
        int ngroups = 128;
        gid_t* groups = malloc(ngroups * sizeof(gid_t));

        getgrouplist(pw->pw_name, pw->pw_gid, groups, &ngroups);

        for( int i = 0; i < ngroups; i++ )
        {
            if( name )
            {
                printf("%s", getgrgid(groups[i])->gr_name);           
            }else
            {
                printf("%d", groups[i]);
            }

            if( zero )
            {
                printf("%c", '\0');
            }else
            {
                printf(" ");
            }
        }

        free(groups);
    }else if( user == 1 )
    {
        if( name )
        {
            printf("%s", pw->pw_name);
        }else
        {
            printf("%d", pw->pw_uid);
        }
    }else
    {
        printf("uid=%d(%s) gid=%d(%s) groups=", pw->pw_uid, pw->pw_name, pw->pw_gid, getgrgid(pw->pw_gid)->gr_name);

        int ngroups = 128;
        gid_t* groups = malloc(ngroups * sizeof(gid_t));

        getgrouplist(pw->pw_name, pw->pw_gid, groups, &ngroups);

        for( int i = 0; i < ngroups; i++ )
        {
            printf("%d(%s)", groups[i], getgrgid(groups[i])->gr_name);
            if( i != ngroups - 1 )
            {
                printf(",");
            }
        }

        free(groups);
    }

    if( zero )
    {
        printf("%c", '\0');
    }else
    {
        printf("\n");
    }
}

// Checks if string is a number
int is_number( char* s )
{
    if( !s || *s == '\0' || isspace(*s) )
    {
        return 0;
    }

    char* p;
    strtod(s, &p);

    return *p == '\0';
}

// Display version text and exit
void show_version( void )
{
    printf("id (\033[1;36mseraph\033[0m coreutils) %s\n", VERSION);

    exit(EXIT_SUCCESS);
}

// Display help text and exit
void show_usage( void )
{
    printf("Usage: id [OPTION(s)] [USER]\n"
           "Print user and group info for current or specified user\n\n"
           " -g, --group   only print group ID\n"
           " -G, --groups  print all group IDs\n"
           " -n, --name    print name instead of ID (for g,G,u)\n"
           " -u, --user    only print effective group ID\n"
           " -z, --zero    delimit entries with NUL instead of newline (for g,G,u)\n"
           "     --help    display this help text and exit\n"
           "     --version display version and exit\n");

    exit(EXIT_SUCCESS);
}
