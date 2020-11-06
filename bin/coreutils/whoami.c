#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <pwd.h>

#include <grp.h>

int main( void )
{
    struct passwd* p = getpwuid(geteuid());

    if( !p )
    {
        return EXIT_FAILURE;
    }

    printf("%s\n", p->pw_name);

    endpwent();

    return EXIT_SUCCESS;
}
