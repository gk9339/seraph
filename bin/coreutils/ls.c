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

struct ls_entry
{
    struct stat st;
    char* filename;
};

int num_places( int n );

int main( int argc, char** argv )
{
    struct dirent* de;
    DIR* dr = NULL;
    char perm[] = "---------\0",
         *filename, *argpath, *path;
    //char datestring[256];
    struct stat st;
    mode_t mode;
    size_t arglen;
    int size_width = 0, width;
    struct ls_entry* entry;
    list_t* ls_list;

    if( argc > 2 )
    {
        printf("Usage: ls <path>\n");
        exit(0);
    }
    if( argc == 1 )
    {
        // Use default directory '.'
        argpath = ".";
    }else
    {
        argpath = argv[1];
    }

    if( lstat(argpath, &st) < 0 )
    {
        printf("\033[0;31mCouldn't stat %s.\n", argpath); // Red
        exit(1);
    }

    if( S_ISDIR(st.st_mode) )
    {
        dr = opendir(argpath);
        arglen = strlen(argpath);

        if( dr == NULL )
        {
            // Opendir failed
            printf("\033[0;31mCouldn't open %s.\n", argpath); // Red
            exit(1);
        }else
        {
            ls_list = list_create();
            while( (de = readdir(dr)) != NULL )
            {
                filename = de->d_name;

                path = malloc(arglen + strlen(filename) + 2);
                sprintf(path, "%s/%s", argpath, filename);
                entry = malloc(sizeof(struct ls_entry));

                if( lstat(path, &entry->st) < 0 )
                {
                    // lstat failed
                    free(path);
                    free(entry);
                    continue;
                }
                free(path);

                entry->filename = malloc(strlen(filename) + 1);
                strcpy(entry->filename, filename);

                width = num_places(entry->st.st_size);
                if( width > size_width )
                {
                    size_width = width;
                }

                list_insert(ls_list, entry);
            }
            closedir(dr);

            foreach(node, ls_list)
            {
                entry = node->value;
                mode = entry->st.st_mode;
                filename = entry->filename;

                perm[0] = (mode & S_IRUSR) ? 'r' : '-';
                perm[1] = (mode & S_IWUSR) ? 'w' : '-';
                perm[2] = (mode & S_IXUSR) ? 'x' : '-';
                perm[3] = (mode & S_IRGRP) ? 'r' : '-';
                perm[4] = (mode & S_IWGRP) ? 'w' : '-';
                perm[5] = (mode & S_IXGRP) ? 'x' : '-';
                perm[6] = (mode & S_IROTH) ? 'r' : '-';
                perm[7] = (mode & S_IWOTH) ? 'w' : '-';
                perm[8] = (mode & S_IXOTH) ? 'x' : '-';
        
                printf("%s %ld %ld %*zdB ", perm, entry->st.st_uid, entry->st.st_gid, size_width, entry->st.st_size);
                if( S_ISBLK(entry->st.st_mode) )
                {
                    printf("\033[1;45m%s", filename); // Magenta BG (Block device)
                }else if( S_ISCHR(entry->st.st_mode) )
                {
                    printf("\033[0;43m%s", filename); // Yellow/Orange BG (Character device)
                
                }else if( S_ISDIR(entry->st.st_mode) )
                {
                    printf("\033[1;34m%s\033[1;32m/", filename); // Blue/Green (Directory)
                }else if( S_ISFIFO(entry->st.st_mode) )
                {
                    printf("\033[1;41m%s", filename); // Red BG (FIFO)
                }else if( S_ISREG(entry->st.st_mode) )
                {
                    if( mode & S_IXUSR || mode & S_IXGRP || mode & S_IXOTH )
                    {
                        printf("\033[0;33m%s\033[1;32m*", filename); // Executable file
                    }else
                    {
                        printf("%s", filename); // Default (Regular file)
                    }
                }else if( S_ISLNK(entry->st.st_mode) )
                {
                    printf("\033[0;33m%s \033[0;34m-> ??", filename); // Default/Blue (Symlink)
                }else if( S_ISSOCK(entry->st.st_mode) )
                {
                    printf("\033[0;31m%s", filename); // Red (Socket)
                }
                printf("\033[0m\n");

                free(entry->filename);
                free(entry);
            }

            list_destroy(ls_list);
        }
    }else
    {
        filename = basename(argpath);

        mode = st.st_mode;

        perm[0] = (mode & S_IRUSR) ? 'r' : '-';
        perm[1] = (mode & S_IWUSR) ? 'w' : '-';
        perm[2] = (mode & S_IXUSR) ? 'x' : '-';
        perm[3] = (mode & S_IRGRP) ? 'r' : '-';
        perm[4] = (mode & S_IWGRP) ? 'w' : '-';
        perm[5] = (mode & S_IXGRP) ? 'x' : '-';
        perm[6] = (mode & S_IROTH) ? 'r' : '-';
        perm[7] = (mode & S_IWOTH) ? 'w' : '-';
        perm[8] = (mode & S_IXOTH) ? 'x' : '-';

        size_width = num_places(st.st_size);

        printf("%s %ld %ld %*zdB ", perm, st.st_uid, st.st_gid, size_width, st.st_size);
        if( S_ISBLK(st.st_mode) )
        {
            printf("\033[1;105m%s", filename); // Magenta BG (Block device)
        }else if( S_ISCHR(st.st_mode) )
        {
            printf("\033[0;43m%s", filename); // Yellow/Orange BG (Character device)

        }else if( S_ISDIR(st.st_mode) )
        {
            printf("\033[1;34m%s\033[1;32m/", filename); // Blue/Green (Directory)
        }else if( S_ISFIFO(st.st_mode) )
        {
            printf("\033[1;41m%s", filename); // Red BG (FIFO)
        }else if( S_ISREG(st.st_mode) )
        {
            printf("%s", filename); // Default (Regular file)
        }else if( S_ISLNK(st.st_mode) )
        {
            printf("\033[0;33m%s \033[0;34m-> ??", filename); // Default/Blue (Symlink)
        }else if( S_ISSOCK(st.st_mode) )
        {
            printf("\033[0;31m%s", filename); // Red (Socket)
        }
        printf("\033[0m\n");
    }

    return 0;
}

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
