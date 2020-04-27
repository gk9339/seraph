#include <unistd.h>

int link( const char* path1, const char* path2 )
{
    return symlink(path1, path2);
}
