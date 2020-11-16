#include <pwd.h>
#include <stdlib.h>
#include <string.h>

#ifdef __is_libk
#include <kernel/fs.h>
#endif

static struct passwd* pw_ent;
static char* pw_blob;

static char *xstrtok( char* line, char* delims)
{
    static char *saveline = NULL;
    char *p;
    int n;

    if( line != NULL )
    {
        saveline = line;
    }

    if( saveline == NULL || *saveline == '\0' )
    {
        return(NULL);
    }

    n = strcspn(saveline, delims);
    p = saveline;

    saveline += n;

    if( *saveline != '\0' )
    {
        *saveline++ = '\0';
    }

    return(p);
}

struct passwd* fgetpwent( FILE* stream )
{
    if( !stream )
    {
        return NULL;
    }

    if( !pw_ent )
    {
        pw_ent = malloc(sizeof(struct passwd));
        pw_blob = malloc(BUFSIZ);
    }

    memset(pw_blob, 0, BUFSIZ);
#ifdef __is_libk
    fs_node_t* pwdb = (fs_node_t*)stream;
    static int pwoff = 0;
    int i = 0;
    while( read_fs(pwdb, pwoff, 1, (uint8_t*)(pw_blob + i)) == 1 )
    {
        pwoff++;
        if( pw_blob[i] == '\n' )
        {
            break;
        }
        i++;
    }
    if( !i )
    {
        return NULL;
    }
#else
    if( !fgets(pw_blob, BUFSIZ, stream) )
    {
        return NULL;
    }
#endif

    if( pw_blob[strlen(pw_blob) - 1] == '\n' )
    {
        pw_blob[strlen(pw_blob) - 1] = '\0';
    }

    char *p, *tok[7];
    p = xstrtok(pw_blob, ":");
    for( int i = 0; i < 7; i++ )
    {
        tok[i] = p;
        p = xstrtok(NULL, ":");
    }

    pw_ent->pw_name = tok[0];
    pw_ent->pw_passwd = tok[1];
    pw_ent->pw_uid = atoi(tok[2]);
    pw_ent->pw_gid = atoi(tok[3]);
    pw_ent->pw_comment = tok[4];
    pw_ent->pw_dir = tok[5];
    pw_ent->pw_shell = tok[6];

    return pw_ent;
}
