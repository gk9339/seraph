#include "sh.h"

void ls( char* argv )
{
    struct dirent* de;
    DIR* dr = NULL;
    char perm[] = "---------\0";
    struct stat st;
    //char datestring[256];
    char* filename;

    char* token = strtok(argv, " ");
    token = strtok(NULL, " ");

    if( token == NULL )
    {
        token = malloc(sizeof(char));
        strcpy(token, ".");
    }else
    {
        token[strlen(token)-1] = '\0';
    }
    dr = opendir(token);

    if( dr == NULL )
    {
        printf("Couldn't open %s.\n", token);
    }else
    {
        while( (de = readdir(dr)) != NULL )
        {
            filename = de->d_name;
            
            char* path = malloc(strlen(token) + strlen(filename) + 2);
            sprintf(path, "%s/%s", token, filename);

            if( lstat(path, &st) < 0 )
            {
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
        }
        closedir(dr);
    }
}
