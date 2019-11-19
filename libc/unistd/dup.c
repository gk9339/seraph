#include <unistd.h>

int dup( int oldfd )
{
    return dup2( oldfd, -1 );
}
