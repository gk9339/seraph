#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/utsname.h>

#define FLAG_SYSNAME  0x01
#define FLAG_NODENAME 0x02
#define FLAG_RELEASE  0x04
#define FLAG_VERSION  0x08
#define FLAG_MACHINE  0x10
#define FLAG_OSNAME   0x20
#define FLAG_ALL (FLAG_SYSNAME|FLAG_NODENAME|FLAG_RELEASE|FLAG_VERSION|FLAG_MACHINE|FLAG_OSNAME)

void show_usage( void );

int main( int argc, char** argv )
{
    struct utsname u;
    int flags = 0;
    int spaces = 0;

    for( int i = 1; i < argc; i++ )
    {
        if( argv[i][0] == '-' )
        {
            char* c = &argv[i][1];
            while(*c)
            {
                switch(*c)
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
                    case 'h':
                    default:
                        show_usage();
                }
                c++;
            }
        }
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

    return 0;
}

void show_usage( void )
{
    fprintf(stderr, "Usage: uname [-asnrvmp]\n"
                    " -a all other flags in order,\n"
                    " -s kernel name\n"
                    " -n nodename / hostname\n"
                    " -r kernel release\n"
                    " -v kernel version\n"
                    " -m architecture\n"
                    " -o operating system name\n"
                    " -p same as -m\n");

    exit(1);
}
