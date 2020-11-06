#include <grp.h>
#include <stdlib.h>
#include <string.h>

static struct group* gr_ent;
static char* gr_blob;

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

struct group* fgetgrent( FILE* stream )
{
    if( !stream )
    {
        return NULL;
    }

    if( !gr_ent )
    {
        gr_ent = malloc(sizeof(struct group));
        gr_blob = malloc(BUFSIZ);
    }

    memset(gr_blob, 0, BUFSIZ);
    if( !fgets(gr_blob, BUFSIZ, stream) )
    {
        return NULL;
    }

    if( gr_blob[strlen(gr_blob) - 1] == '\n' )
    {
        gr_blob[strlen(gr_blob) - 1] = '\0';
    }

    char *p, *tok[64] = { NULL };
    p = xstrtok(gr_blob, ":");
    int i = 0;
    tok[i] = p;
    while( (p = xstrtok(NULL, ":")) )
    {
        i++;
        tok[i] = p;
    }

    gr_ent->gr_mem = malloc((i) * 32 * sizeof(char));
    gr_ent->gr_name = tok[0];
    gr_ent->gr_gid = atoi(tok[1]);

    for( int j = 0; j < i - 1; j++ )
    {
        gr_ent->gr_mem[j] = tok[j + 2];
    }
    if( i )
    {
        gr_ent->gr_mem[i-1] = NULL;
    }

    return gr_ent;
}
