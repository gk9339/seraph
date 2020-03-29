#include <sys/types.h>
#include <dirent.h>
#include <sys/stat.h>
#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main( int argc, char** argv )
{
    struct dirent* de;
    DIR* dr = NULL;
    char perm[] = "---------\0";
    struct stat st;
    //char datestring[256];
    char* filename;
    char* argpath;
    size_t arglen;

    if( argc > 2 )
    {
        printf("Usage: ls <path>\n");
        exit(0);
    }
    if( argc == 1 )
    {
        //use default directory '.'
        argpath = ".";
    }else
    {
        argpath = argv[1];
    }

    dr = opendir(argpath);
    arglen = strlen(argpath);

    if( dr == NULL )
    {
        //opendir failed
        printf("Couldn't open %s.\n", argpath);
        exit(1);
    }else
    {
        while( (de = readdir(dr)) != NULL )
        {
            filename = de->d_name;
            
            char* path = malloc(arglen + strlen(filename) + 2);
            sprintf(path, "%s/%s", argpath, filename);

            if( lstat(path, &st) < 0 )
            {
                //lstat failed
                free(path);
                continue;
            }

            mode_t mode = st.st_mode;

            perm[0] = (mode & S_IRUSR) ? 'r' : '-';
            perm[1] = (mode & S_IWUSR) ? 'w' : '-';
            perm[2] = (mode & S_IXUSR) ? 'x' : '-';
            perm[3] = (mode & S_IRGRP) ? 'r' : '-';
            perm[4] = (mode & S_IWGRP) ? 'w' : '-';
            perm[5] = (mode & S_IXGRP) ? 'x' : '-';
            perm[6] = (mode & S_IROTH) ? 'r' : '-';
            perm[7] = (mode & S_IWOTH) ? 'w' : '-';
            perm[8] = (mode & S_IXOTH) ? 'x' : '-';
			
            printf("%s %ld %ld %ld %s", perm, st.st_uid, st.st_gid, st.st_size, filename);
            if( S_ISDIR(st.st_mode) )
            {
                printf("/");
            }
            printf("\n");

            free(path);
        }
        closedir(dr);
    }

    return 0;
}
