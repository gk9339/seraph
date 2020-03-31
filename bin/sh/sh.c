#include <sys/wait.h>
#include <sys/types.h>
#include <unistd.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>

#define RL_BUFSIZE 1024
#define TOK_BUFSIZE 64
#define TOK_DELIM " \t\r\n\a"

int sh_cd( char** args );
int sh_help( char** args );
int sh_exit( char** args );

// Builtin function names
char* builtin_str[] =
{
    "cd",
    "help",
    "exit",
};

// Builtin function descriptions
char* builtin_desc[] =
{
    "[dir] - Change current working directory",
    "- Display this help prompt",
    "- Exit shell",
};

// Builtin function pointers
int (*builtin_func[])( char** ) =
{
    &sh_cd,
    &sh_help,
    &sh_exit,
};

int sh_num_builtins()
{
    return sizeof(builtin_str) / sizeof(char*);
}

// Change directory
int sh_cd( char** args )
{
    if( args[1] == NULL )
    {
        fprintf(stderr, "sh: expected argument to cd\n");
    }else
    {
        // No chdr yet
        /*if( chdr(args[1]) != 0 )
        {
            perror("sh");
        }*/
    }

    return 1;
}

// Display help text
int sh_help( char** args __attribute__((unused)) )
{
    printf("seraph shell (/bin/sh)\n\n");
    printf("Builtin commands:\n");

    for( int i = 0; i < sh_num_builtins(); i++ )
    {
        printf("\t%s %s\n", builtin_str[i], builtin_desc[i]);
    }

    return 1;
}

// Exit shell
int sh_exit( char** args __attribute__((unused)) )
{
    return 0;
}

// Fork and execvp, wait for child to exit
int sh_launch( char** args )
{
    pid_t pid;
    int status;

    pid = fork();
    if( pid == 0 )
    {
        // Child process
        if( execvp(args[0], args) == -1 )
        {
            printf("sh: Command not found\n");
        }
        exit(EXIT_FAILURE);
    }else if( pid < 0 )
    {
        // Fork error
        perror("sh");
    }else
    {
        // Parent process
        do{
            waitpid(pid, &status, WUNTRACED);
        }while( !WIFEXITED(status) && !WIFSIGNALED(status) );
    }

    return 1;
}

// Check if command is builtin, if not run sh_launch
int sh_execute( char** args )
{
    if( args[0] == NULL )
    {
        return 1;
    }

    for( int i = 0; i < sh_num_builtins(); i++ )
    {
        if( strcmp(args[0], builtin_str[i]) == 0 )
        {
            return (*builtin_func[i])(args);
        }
    }

    return sh_launch(args);
}

// Read line of input from stdio, replace \n with \0
char* sh_readline( void )
{
    int bufsize = RL_BUFSIZE;
    int position = 0;
    char* buffer = malloc(sizeof(char) * bufsize);
    int c;

    if( !buffer )
    {
        fprintf(stderr, "sh: malloc error\n");
        exit(EXIT_FAILURE);
    }

    while( 1 )
    {
        c = getchar();

        if( c == EOF )
        {
            exit(EXIT_SUCCESS);
        }else if( c == '\n' )
        {
            buffer[position] = '\0';
            return buffer;
        }else
        {
            buffer[position] = c;
        }
        position++;

        if( position >= bufsize )
        {
            bufsize += RL_BUFSIZE;
            buffer = realloc(buffer, bufsize);
            if( !buffer )
            {
                fprintf(stderr, "sh: malloc error\n");
                exit(EXIT_FAILURE);
            }
        }
    }
}

// Split line into different strings, return array of arguments
char** sh_splitline( char* line )
{
    int bufsize = TOK_BUFSIZE;
    int position = 0;
    char** tokens = malloc(bufsize * sizeof(char*));
    char* token, **tokens_backup;

    if( !tokens )
    {
        fprintf(stderr, "sh: malloc error\n");
        exit(EXIT_FAILURE);
    }

    token = strtok(line, TOK_DELIM);
    while( token != NULL )
    {
        tokens[position] = token;
        position++;

        if( position >= bufsize )
        {
            bufsize += TOK_BUFSIZE;
            tokens_backup = tokens;
            tokens = realloc(tokens, bufsize * sizeof(char*));
            if( !tokens )
            {
                free(tokens_backup);
                fprintf(stderr, "sh: malloc error\n");
                exit(EXIT_FAILURE);
            }
        }

        token = strtok(NULL, TOK_DELIM);
    }

    tokens[position] = NULL;
    return tokens;
}

// Main shell loop: prompt, readline, splitline, execute
void sh_loop( void )
{
    char* line;
    char** args;
    int status;

    do{
        printf("> ");
        fflush(stdout);
        line = sh_readline(); // Read line from stdin
        args = sh_splitline(line); // Split line into tokens
        status = sh_execute(args); // Execute tokenized input 

        free(line);
        free(args);
    }while( status );
}

int main( void )
{
    // TODO: setup
    setlinebuf(stdout);

    // Main shell loop
    sh_loop();

    // TODO: cleanup
    return EXIT_SUCCESS;
}
