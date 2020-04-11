#include <sys/wait.h>
#include <sys/types.h>
#include <unistd.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <getopt.h>
#include <signal.h>
#include <unistd.h>

#define VERSION "0.3"

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

int exit_sh = 0;
pid_t sh_pgid;

void sh_loop( void ); // Main shell loop: prompt, readline, splitline, execute
char* sh_readline( void ); // Read line of input from stdio, replace \n with \0
char** sh_splitline( char* line ); // Split line into different strings, return array of arguments
int sh_execute( char** args ); // Check if command is builtin, if not run sh_launch
int sh_launch( char** args ); // Fork and execvp, wait for child to exit
int sh_num_builtins( void ); // Number of builtin functions
void show_version( void ); // Display version text and exit
void show_usage( void ); // Display help text and exit

int main( int argc, char** argv )
{
    // TODO: setup

    int c;

    if( argc != 1 )
    {
        while( 1 )
        {
            static struct option long_options[] =
            {
                {"version", no_argument, 0, 'v'},
                {"help", no_argument, 0, 'h'},
                {0, 0, 0, 0}
            };
 
            int option_index = 0;
 
            c = getopt_long(argc, argv, "", long_options, &option_index);
 
            if( c == -1 )
            {
                break;
            }
 
            switch( c )
            {
                case 'v':
                    show_version();
                    __builtin_unreachable();
                case '?':
                    fprintf(stderr, "Try 'sh --help'\n");
                    exit(EXIT_FAILURE);
                case 'h':
                default:
                    show_usage();
                    __builtin_unreachable();
            }
        }
    }

    if( getenv("PATH") == NULL )
    {
        putenv("PATH=/bin");
    }
    setlinebuf(stdout);

    sh_pgid = getpgid(0);

    // Main shell loop
    sh_loop();

    // TODO: cleanup
    return EXIT_SUCCESS;
}

// Main shell loop: prompt, readline, splitline, execute
void sh_loop( void )
{
    char* line;
    char** args;
    int status = 0;
    char cwd[1024];

    do{
        getcwd(cwd, 1023);
        printf("\033[1;33m%s", cwd);
        if( status && WIFEXITED(status) )
        {
            printf("\033[0;41m\u2191%d\033[0m", WEXITSTATUS(status));
        }else if( status && WIFSIGNALED(status) )
        {
            printf("\033[0;41m\u2191%s(%d)\033[0m", sys_signame[WTERMSIG(status)], WTERMSIG(status));
        }
        printf("\033[1;32m$\033[0m ");
        fflush(stdout);
        line = sh_readline(); // Read line from stdin
        args = sh_splitline(line); // Split line into tokens
        status = sh_execute(args); // Execute tokenized input 

        free(line);
        free(args);
    }while( !exit_sh );
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
        fprintf(stderr, "\033[0;31msh: malloc error\n"); // Red
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
                fprintf(stderr, "\033[0;31msh: malloc error\n"); // Red
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
        fprintf(stderr, "\033[0;31msh: malloc error\n"); // Red
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
                fprintf(stderr, "\033[0;31msh: malloc error\n"); // Red
                exit(EXIT_FAILURE);
            }
        }

        token = strtok(NULL, TOK_DELIM);
    }

    tokens[position] = NULL;
    return tokens;
}

// Check if command is builtin, if not run sh_launch
int sh_execute( char** args )
{
    if( args[0] == NULL )
    {
        return 0;
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

// Fork and execvp, wait for child to exit
int sh_launch( char** args )
{
    pid_t pid;
    int status = 0;

    pid = fork();
    if( pid == 0 )
    {
        // Child process
        setpgid(0, 0);
        tcsetpgrp(STDIN_FILENO, getpid());
        if( execvp(args[0], args) == -1 )
        {
            printf("\033[0;31msh: Command not found\n"); // Red
        }
        exit(EXIT_FAILURE);
    }else if( pid < 0 )
    {
        // Fork error
        perror("\033[0;31msh"); // Red
    }else
    {
        // Parent process
        do{
            waitpid(pid, &status, WUNTRACED);
        }while( !WIFEXITED(status) && !WIFSIGNALED(status) );

        tcsetpgrp(STDIN_FILENO, sh_pgid);
    }

    return status;
}

// Number of builtin functions
int sh_num_builtins( void )
{
    return sizeof(builtin_str) / sizeof(char*);
}

// Change directory
int sh_cd( char** args )
{
    if( args[1] == NULL )
    {
        fprintf(stderr, "\033[0;31msh: expected argument to cd\n"); // Red
    }else
    {
        if( chdir(args[1]) != 0 )
        {
            perror("\033[0;31msh"); // Red
        }
    }

    return 0;
}

// Display help text
int sh_help( char** args __attribute__((unused)) )
{
    printf("\033[1;36mseraph\033[0m shell (/bin/sh)\n\n");
    printf("Builtin commands:\n");

    for( int i = 0; i < sh_num_builtins(); i++ )
    {
        printf("\t%s %s\n", builtin_str[i], builtin_desc[i]);
    }

    return 0;
}

// Exit shell
int sh_exit( char** args __attribute__((unused)) )
{
    exit_sh = 1;

    return 0;
}

// Display version text and exit
void show_version( void )
{
    printf("\033[1;36mseraph\033[0m sh %s\n", VERSION);

    exit(EXIT_SUCCESS);
}

// Display help text and exit
void show_usage( void )
{
    printf("Usage sh [OPTION(s)]\n"
           "     --version display version text and exit\n"
           "     --help display this help text and exit\n");

    exit(EXIT_SUCCESS);
}
