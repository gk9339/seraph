#include <pwd.h>
#include <stdlib.h>
#include <string.h>

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
    if( !fgets(pw_blob, BUFSIZ, stream) )
    {
        return NULL;
    }

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
