#include <unistd.h>

int main( int argc, char** argv )
{
    char* arg[] = { NULL };
    char* env[] = { NULL };
    execve("bin/terminal", arg, env);
    
    return 0;
}
