#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <getopt.h>
#include <sys/utsname.h>

#define VERSION "0.3"

#define FLAG_SYSNAME  0x01
#define FLAG_NODENAME 0x02
#define FLAG_RELEASE  0x04
#define FLAG_VERSION  0x08
#define FLAG_MACHINE  0x10
#define FLAG_OSNAME   0x20
#define FLAG_ALL (FLAG_SYSNAME|FLAG_NODENAME|FLAG_RELEASE|FLAG_VERSION|FLAG_MACHINE|FLAG_OSNAME)

int flags = 0;

void parse_args( int argc, char** argv ); // parse args, setting flags, or calling version/help functions
void show_version( void ); // Display version text and exit
void show_usage( void ); // Display help text and exit

int main( int argc, char** argv )
{
    struct utsname u;
    int spaces = 0;

    parse_args(argc, argv);

    if( argc > optind )
    {
        fprintf(stderr, "uname: unknown extra argument '%s'\n"
                        "Try 'uname --help'\n", argv[optind]);
        exit(EXIT_FAILURE);
    }

    if( !flags )
    {
        flags = FLAG_SYSNAME;
    }

    uname(&u);
    
    if( flags & FLAG_SYSNAME )
    {
        if( spaces++) printf(" ");
        printf("%s", u.sysname);
    }
    if( flags & FLAG_NODENAME )
    {
        if( spaces++) printf(" ");
        printf("%s", u.nodename);
    }
    if( flags & FLAG_RELEASE )
    {
        if( spaces++) printf(" ");
        printf("%s", u.release);
    }
    if( flags & FLAG_VERSION )
    {
        if( spaces++) printf(" ");
        printf("%s", u.version);
    }
    if( flags & FLAG_MACHINE )
    {
        if( spaces++) printf(" ");
        printf("%s", u.machine);
    }
    if( flags & FLAG_OSNAME )
    {
        if( spaces++) printf(" ");
        printf("%s", "seraph");
    }

    printf("\n");

    return EXIT_SUCCESS;
}

// parse args, setting flags, or calling version/help functions
void parse_args( int argc, char** argv )
{
    int c;

    while( 1 )
    {
        static struct option long_options[] =
        {
            {"all",              no_argument, 0, 'a'},
            {"kernel-name",      no_argument, 0, 's'},
            {"nodename",         no_argument, 0, 'n'},
            {"kernel-release",   no_argument, 0, 'r'},
            {"kernel-version",   no_argument, 0, 'v'},
            {"machine",          no_argument, 0, 'm'},
            {"operating-system", no_argument, 0, 'o'},
            {"processor",        no_argument, 0, 'p'},
            {"help",             no_argument, 0, 'h'},
            {"version",          no_argument, 0, 'e'},
            {0, 0, 0, 0}
        };

        int option_index = 0;

        c = getopt_long(argc, argv, "asnrvmoph", long_options, &option_index);
        
        if( c == -1 )
        {
            break;
        }

        switch( c )
        {
            case 'a':
                flags |= FLAG_ALL;
                break;
            case 's':
                flags |= FLAG_SYSNAME;
                break;
            case 'n':
                flags |= FLAG_NODENAME;
                break;
            case 'r':
                flags |= FLAG_RELEASE;
                break;
            case 'v':
                flags |= FLAG_VERSION;
                break;
            case 'm':
            case 'p':
                flags |= FLAG_MACHINE;
                break;
            case 'o':
                flags |= FLAG_OSNAME;
                break;
            case 'e':
                show_version();
                __builtin_unreachable();
            case '?':
                fprintf(stderr,"Try 'uname --help'\n");
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
    printf("uname (\033[1;36mseraph\033[0m coreutils) %s\n", VERSION);

    exit(EXIT_SUCCESS);
}

// Display help text and exit
void show_usage( void )
{
    printf("Usage: uname [OPTION(s)]\n"
           "Print system information. No option implies -s\n\n"
           " -a, --all              print all other flags in order,\n"
           " -s, --kernel-name      print kernel name\n"
           " -n, --nodename         print nodename / hostname\n"
           " -r, --kernel-release   print kernel release\n"
           " -v, --kernel-version   print kernel version\n"
           " -m, --machine          print architecture\n"
           " -o, --operating-system print operating system name\n"
           " -p, --processor        same as -m\n"
           " -h, --help             display this help text and exit\n"
           "     --version          display version and exit\n");

    exit(EXIT_SUCCESS);
}
